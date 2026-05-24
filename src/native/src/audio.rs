use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Stream,
};
use napi_derive::napi;
use pyo3::{exceptions::PyRuntimeError, prelude::*};

struct Pending {
    start_time: f64,
    /// Interleaved samples: ch0_s0, ch1_s0, ..., ch0_s1, ch1_s1, ...
    samples: Vec<f32>,
}

struct Shared {
    playing: Vec<f32>,
    play_pos: usize,
    /// Scheduled entries sorted by start_time ascending
    pending: Vec<Pending>,
    /// Wall clock timestamp of playback time 0
    playback_start: Option<Instant>,
    exit: bool,
}

impl Shared {
    fn current_secs(&self) -> f64 {
        self.playback_start
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }
}

#[pyclass]
#[napi]
pub struct AudioManager {
    channels: u16,
    sample_rate: u32,
    bit_depth: u32,
    shared: Arc<Mutex<Shared>>,
    stream: Option<Stream>,
    monitor: Option<thread::JoinHandle<()>>,
}

impl AudioManager {
    fn build_stream(
        channels: u16,
        sample_rate: u32,
        shared: Arc<Mutex<Shared>>,
    ) -> PyResult<Stream> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| PyRuntimeError::new_err("no default audio output device"))?;

        let config = cpal::StreamConfig {
            channels,
            sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        let shared_cb = Arc::clone(&shared);
        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _| {
                    let mut st = match shared_cb.try_lock() {
                        Ok(g) => g,
                        Err(_) => {
                            for s in out.iter_mut() {
                                *s = 0.0;
                            }
                            return;
                        }
                    };
                    for s in out.iter_mut() {
                        if st.play_pos < st.playing.len() {
                            *s = st.playing[st.play_pos];
                            st.play_pos += 1;
                        } else {
                            *s = 0.0;
                        }
                    }
                },
                |err| eprintln!("[AudioManager] cpal error: {err}"),
                None,
            )
            .map_err(|e| PyRuntimeError::new_err(format!("build_output_stream failed: {e}")))?;

        stream
            .play()
            .map_err(|e| PyRuntimeError::new_err(format!("stream.play failed: {e}")))?;

        Ok(stream)
    }

    fn spawn_monitor(shared: Arc<Mutex<Shared>>) -> thread::JoinHandle<()> {
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(1));

            let mut st = match shared.lock() {
                Ok(g) => g,
                Err(_) => break,
            };

            if st.exit {
                break;
            }

            let now = st.current_secs();

            let due: Vec<usize> = st
                .pending
                .iter()
                .enumerate()
                .filter(|(_, p)| p.start_time <= now)
                .map(|(i, _)| i)
                .collect();

            if due.is_empty() {
                continue;
            }

            // Pick the entry with the latest start_time among those that are due
            let best = *due
                .iter()
                .max_by(|&&a, &&b| {
                    st.pending[a]
                        .start_time
                        .total_cmp(&st.pending[b].start_time)
                })
                .unwrap();

            let entry = st.pending.remove(best);
            // Discard any remaining overdue entries
            st.pending.retain(|p| p.start_time > now);
            st.playing = entry.samples;
            st.play_pos = 0;
        })
    }
}

#[pymethods]
#[napi]
impl AudioManager {
    /// Create an AudioManager.
    ///
    /// - `channels`: number of output channels
    /// - `sample_rate`: samples per second (Hz)
    /// - `bit_depth`: bits per sample (informational; cpal plays f32 directly)
    #[new]
    #[napi(constructor)]
    pub fn new(channels: u16, sample_rate: u32, bit_depth: u32) -> Result<Self> {
        let shared = Arc::new(Mutex::new(Shared {
            playing: Vec::new(),
            play_pos: 0,
            pending: Vec::new(),
            playback_start: None,
            exit: false,
        }));
        let stream = Self::build_stream(channels, sample_rate, Arc::clone(&shared))?;
        let monitor = Self::spawn_monitor(Arc::clone(&shared));
        Ok(Self {
            channels,
            sample_rate,
            bit_depth,
            shared,
            stream: Some(stream),
            monitor: Some(monitor),
        })
    }

    #[getter]
    #[napi]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Changing channels rebuilds the output stream.
    #[setter]
    #[napi]
    pub fn set_channels(&mut self, value: u16) -> Result<()> {
        let stream = Self::build_stream(value, self.sample_rate, Arc::clone(&self.shared))?;
        self.channels = value;
        self.stream = Some(stream);
        Ok(())
    }

    #[getter]
    #[napi]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Changing sample_rate rebuilds the output stream.
    #[setter]
    #[napi]
    pub fn set_sample_rate(&mut self, value: u32) -> Result<()> {
        let stream = Self::build_stream(self.channels, value, Arc::clone(&self.shared))?;
        self.sample_rate = value;
        self.stream = Some(stream);
        Ok(())
    }

    #[getter]
    #[napi]
    pub fn bit_depth(&self) -> u32 {
        self.bit_depth
    }

    #[setter]
    #[napi]
    pub fn set_bit_depth(&mut self, value: u32) {
        self.bit_depth = value;
    }

    /// Stack audio data for playback at `start_time` seconds from the playback clock.
    ///
    /// `data` shape: `[[ch0_s0, ch0_s1, ...], [ch1_s0, ch1_s1, ...], ...]`
    ///
    /// The playback clock starts on the first call to `stack_audio` and is reset by `stop()`.
    /// If `start_time` has already elapsed, playback begins within ~1 ms. If another audio
    /// is playing at `start_time`, it is interrupted and replaced.
    pub fn stack_audio(&mut self, data: Vec<Vec<f32>>, start_time: f64) -> Result<()> {
        if data.is_empty() {
            bail!("data must have at least one channel");
        }
        let n_samples = data[0].len();
        if data.iter().any(|ch| ch.len() != n_samples) {
            bail!("all channels must have the same number of samples");
        }

        let n_chs = data.len();
        let mut interleaved = Vec::with_capacity(n_chs * n_samples);
        for s in 0..n_samples {
            for ch in &data {
                interleaved.push(ch[s]);
            }
        }

        let mut st = self
            .shared
            .lock()
            .map_err(|_| PyRuntimeError::new_err("mutex poisoned"))?;

        if st.playback_start.is_none() {
            st.playback_start = Some(Instant::now());
        }

        st.pending.push(Pending {
            start_time,
            samples: interleaved,
        });
        st.pending
            .sort_by(|a, b| a.start_time.total_cmp(&b.start_time));

        Ok(())
    }

    /// Stop playback immediately and clear all stacked audio. Resets the playback clock.
    #[napi]
    pub fn stop(&mut self) -> Result<()> {
        let mut st = self.shared.lock().map_err(|_| anyhow!("mutex poisoned"))?;
        st.playing.clear();
        st.play_pos = 0;
        st.pending.clear();
        st.playback_start = None;
        Ok(())
    }

    /// Elapsed seconds since the playback clock started (0.0 before first `stack_audio`).
    #[getter]
    #[napi]
    pub fn current_time(&self) -> Result<f64> {
        let st = self.shared.lock().map_err(|_| anyhow!("mutex poisoned"))?;
        Ok(st.current_secs())
    }
}

impl Drop for AudioManager {
    fn drop(&mut self) {
        if let Ok(mut st) = self.shared.lock() {
            st.exit = true;
        }
        drop(self.stream.take());
        if let Some(h) = self.monitor.take() {
            let _ = h.join();
        }
    }
}

#[pymodule(name = "audio")]
pub mod audio_register {
    #[pymodule_export]
    use super::AudioManager;
}
