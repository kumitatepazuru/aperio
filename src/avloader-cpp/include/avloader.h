#pragma once
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// ─── Video API ───────────────────────────────────────────────────────────────

typedef void* AvLoaderHandle;

// Open a video file. Tries hardware decoders first; falls back to software.
// Returns NULL on failure.
AvLoaderHandle avloader_video_open(const char* path);
void           avloader_video_close(AvLoaderHandle h);

// Immutable properties (safe to read from any thread without locking)
int    avloader_video_width(AvLoaderHandle h);
int    avloader_video_height(AvLoaderHandle h);
int    avloader_video_pixel_format(AvLoaderHandle h);   // AVPixelFormat value (SW format)
// AVColorRange: 1=MPEG/limited (16-235), 2=JPEG/full (0-255), 0=unspecified (treat as limited)
int    avloader_video_color_range(AvLoaderHandle h);
double  avloader_video_native_fps(AvLoaderHandle h);
int64_t avloader_video_frame_count(AvLoaderHandle h); // exact frame count (always > 0 for seekable files)

// YUV plane info
int  avloader_video_yuv_plane_count(AvLoaderHandle h);
void avloader_video_yuv_plane_info(AvLoaderHandle h, int plane_idx,
                                    int* tex_width, int* tex_height,
                                    int* bytes_per_texel);

// Decode frame_num (1-based, relative to target_fps) into pre-allocated plane buffers.
// target_fps: frame rate used for frame-number ↔ timestamp mapping.
// bytes_per_row[i] = tex_width[i] * bytes_per_texel[i]  (compact, no padding).
// Thread-safe: serialised by an internal mutex.
// Returns 0 on success, -1 on failure.
int avloader_video_decode_frame(AvLoaderHandle h, uint64_t frame_num, double target_fps,
                                 int num_planes, uint8_t** plane_bufs,
                                 const int* bytes_per_row);

// Decode frame_num to RGB24 (channels=3) or RGBA32 (channels=4).
// target_fps: frame rate used for frame-number ↔ timestamp mapping.
// out_buf must hold at least width * height * channels bytes.
// Thread-safe: serialised by an internal mutex.
// Returns 0 on success, -1 on failure.
int avloader_video_decode_frame_rgb(AvLoaderHandle h, uint64_t frame_num, double target_fps,
                                     uint8_t* out_buf, size_t buf_size, int channels);

// ─── Audio API ───────────────────────────────────────────────────────────────

typedef void* AvAudioHandle;

// Open an audio stream (audio file or video file's audio track).
// Returns NULL on failure.
AvAudioHandle avloader_audio_open(const char* path);
void          avloader_audio_close(AvAudioHandle h);

// Immutable properties (safe to read from any thread without locking)
int     avloader_audio_channels(AvAudioHandle h);
double  avloader_audio_duration(AvAudioHandle h);
int64_t avloader_audio_bitrate(AvAudioHandle h);
int     avloader_audio_sampling_rate(AvAudioHandle h);

// Decode audio from `time` seconds for `duration` seconds.
// out_buf layout: planar float32 — [ch0_s0…ch0_sN, ch1_s0…ch1_sN, …]
// buf_samples_per_channel: float capacity per channel.
// Returns actual samples-per-channel written on success, -1 on failure.
int64_t avloader_audio_get_audio(AvAudioHandle h, double time, double duration,
                                  float* out_buf, int64_t buf_samples_per_channel);

#ifdef __cplusplus
}
#endif
