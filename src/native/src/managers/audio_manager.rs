use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Stream,
};

struct AudioBuffer {
    /// Interleaved f32 samples. data[0] corresponds to sample index `buffer_start`.
    data: Vec<f32>,
    /// Absolute sample index of data[0].
    buffer_start: u64,
    /// Absolute sample index of the next frame the CPAL callback will output.
    play_pos: u64,
    /// False only when stop() has been called; true once stack_audio triggers auto-play.
    is_playing: bool,
}

pub struct AudioManager {
    pub(super) channels: u16,
    pub(super) sample_rate: u32,
    pub(super) bit_depth: u32,
    buffer: Arc<Mutex<AudioBuffer>>,
    stream: Option<Stream>,
}

impl AudioManager {
    fn build_stream(
        channels: u16,
        sample_rate: u32,
        buffer: Arc<Mutex<AudioBuffer>>,
    ) -> Result<Stream> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no default audio output device"))?;

        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate::from(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer_cb = Arc::clone(&buffer);
        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _| {
                    let mut buf = match buffer_cb.lock() {
                        Ok(g) => g,
                        Err(_) => {
                            out.fill(0.0);
                            return;
                        }
                    };
                    if !buf.is_playing {
                        out.fill(0.0);
                        return;
                    }

                    let n_frames = out.len() / channels as usize;
                    let rel = buf.play_pos.saturating_sub(buf.buffer_start) as usize;
                    let data_start = rel * channels as usize;
                    let avail = buf.data.len().saturating_sub(data_start);
                    let to_copy = out.len().min(avail);

                    if to_copy > 0 {
                        out[..to_copy].copy_from_slice(&buf.data[data_start..data_start + to_copy]);
                    }
                    out[to_copy..].fill(0.0);

                    buf.play_pos += (to_copy / channels as usize) as u64;

                    // If we ran out of data (underrun), stop playback completely
                    if to_copy < out.len() {
                        buf.is_playing = false;
                        buf.data.clear();
                        buf.buffer_start = 0;
                        buf.play_pos = 0;
                    }
                },
                |err| log::error!("[AudioManager] cpal error: {err}"),
                None,
            )
            .map_err(|e| anyhow!("build_output_stream failed: {e}"))?;

        stream
            .play()
            .map_err(|e| anyhow!("stream.play failed: {e}"))?;

        Ok(stream)
    }

    pub fn new(channels: u16, sample_rate: u32, bit_depth: u32) -> Result<Self> {
        let buffer = Arc::new(Mutex::new(AudioBuffer {
            data: Vec::new(),
            buffer_start: 0,
            play_pos: 0,
            is_playing: false,
        }));
        let stream = Self::build_stream(channels, sample_rate, Arc::clone(&buffer))?;
        Ok(Self {
            channels,
            sample_rate,
            bit_depth,
            buffer,
            stream: Some(stream),
        })
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn set_channels(&mut self, value: u16) -> Result<()> {
        {
            let mut buf = self.buffer.lock().map_err(|_| anyhow!("mutex poisoned"))?;
            buf.data.clear();
            buf.buffer_start = 0;
            buf.play_pos = 0;
            buf.is_playing = false;
        }
        let stream = Self::build_stream(value, self.sample_rate, Arc::clone(&self.buffer))?;
        self.channels = value;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn set_sample_rate(&mut self, value: u32) -> Result<()> {
        {
            let mut buf = self.buffer.lock().map_err(|_| anyhow!("mutex poisoned"))?;
            buf.data.clear();
            buf.buffer_start = 0;
            buf.play_pos = 0;
            buf.is_playing = false;
        }
        let stream = Self::build_stream(self.channels, value, Arc::clone(&self.buffer))?;
        self.sample_rate = value;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn bit_depth(&self) -> u32 {
        self.bit_depth
    }

    pub fn set_bit_depth(&mut self, value: u32) {
        self.bit_depth = value;
    }

    /// Stack audio data starting at `start_time` (absolute sample index).
    ///
    /// If playback is stopped, auto-starts from `start_time`.
    /// Overlapping regions overwrite previously stacked data at the same position.
    ///
    /// `samples`: interleaved f32 (ch0_s0, ch1_s0, ..., ch0_s1, ch1_s1, ...)
    /// `n_channels`: number of channels (for validation)
    /// `start_time`: absolute sample index of samples[0]
    pub fn stack_audio(
        &mut self,
        samples: Vec<f32>,
        n_channels: usize,
        start_time: u64,
    ) -> Result<()> {
        if n_channels == 0 {
            bail!("n_channels must be > 0");
        }
        if samples.len() % n_channels != 0 {
            bail!(
                "samples.len() ({}) must be divisible by n_channels ({})",
                samples.len(),
                n_channels
            );
        }

        let n_frames = samples.len() / n_channels;
        if n_frames == 0 {
            return Ok(());
        }

        let mut buf = self.buffer.lock().map_err(|_| anyhow!("mutex poisoned"))?;

        // Auto-play: if stopped, jump to start_time and begin playback.
        if !buf.is_playing {
            buf.play_pos = start_time;
            buf.buffer_start = start_time;
            buf.data.clear();
            buf.is_playing = true;
        }

        // Skip samples that have already been played.
        let end_time = start_time + n_frames as u64;
        let effective_start = start_time.max(buf.play_pos);
        if effective_start >= end_time {
            return Ok(()); // entirely in the past
        }
        let src_skip = (effective_start - start_time) as usize * n_channels;
        let src = &samples[src_skip..];
        let effective_end = effective_start + (src.len() / n_channels) as u64;

        // Trim fully-played data from the front to keep the buffer compact.
        // Only trim when more than half is already consumed to amortize the O(n) drain.
        if buf.data.is_empty() {
            buf.buffer_start = buf.play_pos;
        } else {
            let trim_frames = buf.play_pos.saturating_sub(buf.buffer_start) as usize;
            if trim_frames > buf.data.len() / n_channels / 2 && trim_frames > 0 {
                let trim_samples = (trim_frames * n_channels).min(buf.data.len());
                buf.data.drain(..trim_samples);
                buf.buffer_start += trim_frames as u64;
            }
        }

        let buf_end = buf.buffer_start + (buf.data.len() / n_channels) as u64;

        if effective_start >= buf_end {
            // New data starts at or after current buffer end.
            if effective_start > buf_end {
                // Fill the gap with silence.
                let gap = (effective_start - buf_end) as usize * n_channels;
                buf.data.extend(std::iter::repeat(0.0f32).take(gap));
            }
            buf.data.extend_from_slice(src);
        } else if effective_end <= buf_end {
            // New data falls entirely within the existing buffer: overwrite in place.
            let write_start = (effective_start - buf.buffer_start) as usize * n_channels;
            buf.data[write_start..write_start + src.len()].copy_from_slice(src);
        } else {
            // Partial overlap: overwrite the existing portion, then extend.
            let write_start = (effective_start - buf.buffer_start) as usize * n_channels;
            let in_buf = (buf_end - effective_start) as usize * n_channels;
            buf.data[write_start..write_start + in_buf].copy_from_slice(&src[..in_buf]);
            buf.data.extend_from_slice(&src[in_buf..]);
        }

        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        let mut buf = self.buffer.lock().map_err(|_| anyhow!("mutex poisoned"))?;
        buf.data.clear();
        buf.buffer_start = 0;
        buf.play_pos = 0;
        buf.is_playing = false;
        Ok(())
    }

    pub fn current_time(&self) -> Result<f64> {
        let buf = self.buffer.lock().map_err(|_| anyhow!("mutex poisoned"))?;
        if !buf.is_playing {
            return Ok(0.0);
        }
        Ok(buf.play_pos as f64 / self.sample_rate as f64)
    }

    /// Returns the number of sample frames buffered ahead of the current playback position.
    pub fn pending_samples(&self) -> Result<u64> {
        let channels = self.channels as usize;
        if channels == 0 {
            return Ok(0);
        }
        let buf = self.buffer.lock().map_err(|_| anyhow!("mutex poisoned"))?;
        let buf_end = buf.buffer_start + (buf.data.len() / channels) as u64;
        Ok(buf_end.saturating_sub(buf.play_pos))
    }
}

impl Drop for AudioManager {
    fn drop(&mut self) {
        // Stop the CPAL stream first so the callback cannot access the buffer after we release it.
        self.stream = None;
    }
}
