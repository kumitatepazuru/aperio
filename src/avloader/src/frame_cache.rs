/// Sequential-prefetch cache with a random-access LRU fallback.
///
/// # Strategy
/// * `prefetch` – a sliding BTreeMap window of the next `fps * 2` frames decoded
///   by a background thread in sequential order (closest-first).
/// * `lru` – an LRU cache (30 frames) populated on **cache misses** (non-sequential
///   seeks), so that repeated seeks to the same position stay fast.
///
/// # Thread safety
/// All shared state lives behind `Arc<Mutex<CacheState>>`. The C++ decoder has
/// its own internal mutex, so `decode_frame` can be called from any thread
/// without additional synchronisation.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use crate::ffi::{avloader_close, avloader_decode_frame, AvLoaderHandle};
use crate::yuv_pipeline::PlaneDesc;
use gpu_util::resource_pool::LruCache;

const LRU_CAPACITY: usize = 30;

// ─── CachedYuvFrame ──────────────────────────────────────────────────────────

pub struct CachedYuvFrame {
    pub planes: Vec<Vec<u8>>,
}

// ─── DecoderRef ──────────────────────────────────────────────────────────────
// Owns the C++ AvLoaderHandle and closes it on drop. Multiple Arc clones can
// exist; avloader_close is called exactly once when the last clone is dropped.

pub(crate) struct DecoderRef(pub(crate) AvLoaderHandle);

// SAFETY: The C++ decoder serialises all calls via an internal std::mutex.
unsafe impl Send for DecoderRef {}
unsafe impl Sync for DecoderRef {}

impl Drop for DecoderRef {
    fn drop(&mut self) {
        unsafe { avloader_close(self.0) };
    }
}

// ─── CacheState ──────────────────────────────────────────────────────────────

struct CacheState {
    /// Sequential prefetch window: frames in (current_frame, current_frame + fps*2].
    prefetch: BTreeMap<u64, Arc<CachedYuvFrame>>,
    /// Random-access LRU: populated on non-sequential cache misses.
    lru: LruCache<u64, Arc<CachedYuvFrame>>,
    /// Frames that failed to decode (skip in future prefetch passes).
    failed: BTreeSet<u64>,

    current_frame: u64,
    fps: f64,
    target_fps: f64,

    /// Frame the main thread urgently needs (set on cache miss).
    priority_frame: Option<u64>,
    /// Set to true when a priority decode attempt failed.
    priority_failed: bool,

    stop: bool,
}

impl CacheState {
    fn prefetch_limit(&self) -> u64 {
        self.current_frame + (self.fps * 2.0).ceil() as u64
    }

    /// Discard frames outside the current sliding window.
    fn trim(&mut self) {
        let lo = self.current_frame;
        let hi = self.prefetch_limit();
        self.prefetch.retain(|&k, _| k > lo && k <= hi);
        self.failed.retain(|&k| k > lo && k <= hi);
    }

    /// Next frame in [current+1, limit] that hasn't been cached or failed.
    fn next_prefetch_target(&self) -> Option<u64> {
        let hi = self.prefetch_limit();
        (self.current_frame + 1..=hi)
            .find(|f| !self.prefetch.contains_key(f) && !self.failed.contains(f))
    }

    fn should_wake_bg(&self) -> bool {
        self.priority_frame.is_some() || self.next_prefetch_target().is_some()
    }
}

// ─── FrameCacheInner ─────────────────────────────────────────────────────────

struct FrameCacheInner {
    state:     Mutex<CacheState>,
    bg_cond:   Condvar,   // wake background thread
    main_cond: Condvar,   // wake main thread (priority decode done)
    decoder:   Arc<DecoderRef>,
    descs:     Vec<PlaneDesc>,
}

// ─── FrameCache ──────────────────────────────────────────────────────────────

pub struct FrameCache {
    inner:      Arc<FrameCacheInner>,
    bg_thread:  Option<thread::JoinHandle<()>>,
}

impl FrameCache {
    pub fn new(decoder: Arc<DecoderRef>, descs: Vec<PlaneDesc>, fps: f64) -> Self {
        let state = Mutex::new(CacheState {
            prefetch:        BTreeMap::new(),
            lru:             LruCache::new(LRU_CAPACITY),
            failed:          BTreeSet::new(),
            current_frame:   0,
            fps,
            target_fps:      fps,
            priority_frame:  None,
            priority_failed: false,
            stop:            false,
        });

        let inner = Arc::new(FrameCacheInner {
            state,
            bg_cond:   Condvar::new(),
            main_cond: Condvar::new(),
            decoder,
            descs,
        });

        let bg_inner = Arc::clone(&inner);
        let bg = thread::spawn(move || bg_worker(bg_inner));

        Self { inner, bg_thread: Some(bg) }
    }

    /// Retrieve frame `frame_num`.
    ///
    /// * **Sequential hit** – returned from the prefetch window (ownership moved out).
    /// * **LRU hit** – returned from the random-access cache.
    /// * **Miss** – background thread is asked to decode it first (blocks until done).
    ///   The decoded frame is also inserted into the LRU for future seeks.
    pub fn get(&self, frame_num: u64, target_fps: f64) -> Option<Arc<CachedYuvFrame>> {
        let inner = &self.inner;
        let mut state = inner.state.lock().unwrap();

        // Flush all cached frames when target_fps changes: frame_num → timestamp
        // mapping differs, so cached frames would correspond to wrong positions.
        if (state.target_fps - target_fps).abs() > f64::EPSILON {
            state.prefetch.clear();
            state.lru = LruCache::new(LRU_CAPACITY);
            state.failed.clear();
            state.target_fps = target_fps;
        }

        let sequential = state.prefetch.contains_key(&frame_num);

        state.current_frame = frame_num;

        // ── Sequential hit ────────────────────────────────────────────────
        if sequential {
            let frame = state.prefetch.remove(&frame_num).unwrap();
            state.trim();
            inner.bg_cond.notify_one();
            return Some(frame);
        }

        // ── LRU hit ───────────────────────────────────────────────────────
        if let Some(frame) = state.lru.get(&frame_num) {
            state.trim();
            inner.bg_cond.notify_one();
            return Some(frame);
        }

        // ── Cache miss: request priority decode ───────────────────────────
        state.priority_frame  = Some(frame_num);
        state.priority_failed = false;
        inner.bg_cond.notify_one();

        let mut state = inner
            .main_cond
            .wait_while(state, |s| {
                !s.stop
                    && !s.prefetch.contains_key(&frame_num)
                    && !s.priority_failed
            })
            .unwrap();

        if state.stop || state.priority_failed {
            state.priority_failed = false;
            return None;
        }

        let frame = state.prefetch.remove(&frame_num)?;

        // Register in LRU so repeated seeks to this position stay fast
        state.lru.insert(frame_num, Arc::clone(&frame));

        state.trim();
        inner.bg_cond.notify_one();
        Some(frame)
    }
}

impl Drop for FrameCache {
    fn drop(&mut self) {
        {
            let mut s = self.inner.state.lock().unwrap();
            s.stop = true;
        }
        self.inner.bg_cond.notify_all();
        if let Some(handle) = self.bg_thread.take() {
            let _ = handle.join();
        }
    }
}

// ─── Background worker ───────────────────────────────────────────────────────

fn bg_worker(inner: Arc<FrameCacheInner>) {
    loop {
        // Wait until there is something to do
        let (frame_to_decode, is_priority, target_fps) = {
            let mut state = inner
                .bg_cond
                .wait_while(inner.state.lock().unwrap(), |s| {
                    !s.stop && !s.should_wake_bg()
                })
                .unwrap();

            if state.stop { break; }

            let target_fps = state.target_fps;

            if let Some(pf) = state.priority_frame {
                // If the frame is already in prefetch (race with sequential decode),
                // just notify the main thread and skip the decode.
                if state.prefetch.contains_key(&pf) {
                    state.priority_frame = None;
                    inner.main_cond.notify_all();
                    continue;
                }
                state.priority_frame = None;
                (pf, true, target_fps)
            } else if let Some(next) = state.next_prefetch_target() {
                (next, false, target_fps)
            } else {
                continue;
            }
        };

        // Decode outside the lock (C++ decoder has its own mutex)
        match decode_one(&inner.decoder, frame_to_decode, &inner.descs, target_fps) {
            Some(frame) => {
                let mut state = inner.state.lock().unwrap();
                state.prefetch.insert(frame_to_decode, Arc::new(frame));
                if is_priority {
                    inner.main_cond.notify_all();
                }
            }
            None => {
                let mut state = inner.state.lock().unwrap();
                if is_priority {
                    state.priority_failed = true;
                    inner.main_cond.notify_all();
                } else {
                    // Mark as failed so we don't retry the same frame endlessly
                    state.failed.insert(frame_to_decode);
                }
            }
        }
    }
}

fn decode_one(
    decoder: &DecoderRef,
    frame_num: u64,
    descs: &[PlaneDesc],
    target_fps: f64,
) -> Option<CachedYuvFrame> {
    let mut planes: Vec<Vec<u8>> = descs.iter().map(|d| vec![0u8; d.total_bytes()]).collect();
    let mut ptrs: Vec<*mut u8>   = planes.iter_mut().map(|v| v.as_mut_ptr()).collect();
    let strides: Vec<i32>        = descs.iter().map(|d| d.bytes_per_row() as i32).collect();

    let ret = unsafe {
        avloader_decode_frame(
            decoder.0,
            frame_num,
            target_fps,
            descs.len() as i32,
            ptrs.as_mut_ptr(),
            strides.as_ptr(),
        )
    };

    if ret < 0 { None } else { Some(CachedYuvFrame { planes }) }
}
