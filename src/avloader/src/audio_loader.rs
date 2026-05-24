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

    /// Decode `duration` seconds of audio starting at `time`.
    ///
    /// Returns a 2-D array where the outer dimension is channels and the inner
    /// dimension is samples: `[[L0, L1, …], [R0, R1, …]]` for stereo.
    pub fn get_audio(&self, time: f64, duration: f64) -> Result<Vec<Vec<f32>>> {
        if self.channels == 0 || self.sampling_rate == 0 {
            bail!("AudioLoader: invalid channel/sample-rate configuration");
        }

        // Allocate one extra sample per channel as a rounding buffer.
        let samples_per_channel = (duration * self.sampling_rate as f64).ceil() as i64 + 1;
        let total = self.channels as usize * samples_per_channel as usize;
        let mut buf = vec![0f32; total];

        let actual = unsafe {
            avloader_audio_get_audio(
                self.handle,
                time,
                duration,
                buf.as_mut_ptr(),
                samples_per_channel,
            )
        };

        if actual < 0 {
            bail!("avloader_audio_get_audio failed (time={time:.3}, duration={duration:.3})");
        }

        let actual = actual as usize;
        let mut result = Vec::with_capacity(self.channels as usize);
        for ch in 0..self.channels as usize {
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
