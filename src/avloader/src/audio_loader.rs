use crate::ffi::*;
use anyhow::{bail, Context, Result};

// ─── AudioLoader ─────────────────────────────────────────────────────────────

pub struct AudioLoader {
    handle: AvAudioHandle,
    channels: u32,
    duration: f64,
    bit_depth: i32,
    sampling_rate: u32,
}

// SAFETY: All C++ decoder calls are serialised by an internal std::mutex.
unsafe impl Send for AudioLoader {}
unsafe impl Sync for AudioLoader {}

impl AudioLoader {
    pub fn new(path: &str) -> Result<Self> {
        let c_path = std::ffi::CString::new(path).context("Audio path contains null byte")?;

        let handle = unsafe { avloader_audio_open(c_path.as_ptr()) };
        if handle.is_null() {
            bail!("avloader_audio_open failed: could not open \"{}\"", path);
        }

        let channels = unsafe { avloader_audio_channels(handle) } as u32;
        let duration = unsafe { avloader_audio_duration(handle) };
        let bit_depth = unsafe { avloader_audio_bit_depth(handle) };
        let sampling_rate = unsafe { avloader_audio_sampling_rate(handle) } as u32;

        Ok(Self {
            handle,
            channels,
            duration,
            bit_depth,
            sampling_rate,
        })
    }

    pub fn get_chs(&self) -> u32 {
        self.channels
    }

    pub fn get_duration(&self) -> f64 {
        self.duration
    }

    pub fn get_bit_depth(&self) -> i32 {
        self.bit_depth
    }

    pub fn get_sampling_rate(&self) -> u32 {
        self.sampling_rate
    }

    /// Decode audio starting at `time_samples` (in `target_sample_rate` units)
    /// for `duration_samples` samples (also in `target_sample_rate` units), resampling to
    /// `target_sample_rate` Hz and remixing to `target_channels` channels via swresample.
    pub fn get_audio(
        &self,
        time_samples: i64,
        duration_samples: i64,
        target_sample_rate: u32,
        target_channels: u32,
    ) -> Result<Vec<Vec<f32>>> {
        if self.channels == 0 || self.sampling_rate == 0 {
            bail!("AudioLoader: invalid channel/sample-rate configuration");
        }

        // duration_samples is already in target-rate units; add 1 for SRC headroom.
        let samples_per_channel = duration_samples + 1;
        let total = target_channels as usize * samples_per_channel as usize;
        let mut buf = vec![0f32; total];

        let actual = unsafe {
            avloader_audio_get_audio(
                self.handle,
                time_samples,
                duration_samples,
                target_sample_rate as i32,
                target_channels as i32,
                buf.as_mut_ptr(),
                samples_per_channel,
            )
        };

        if actual < 0 {
            bail!("avloader_audio_get_audio failed (time_samples={time_samples}, duration_samples={duration_samples})");
        }

        let actual = actual as usize;
        let mut result = Vec::with_capacity(target_channels as usize);
        for ch in 0..target_channels as usize {
            let start = ch * samples_per_channel as usize;
            result.push(buf[start..start + actual].to_vec());
        }
        Ok(result)
    }
}

impl Drop for AudioLoader {
    fn drop(&mut self) {
        unsafe { avloader_audio_close(self.handle) };
    }
}
