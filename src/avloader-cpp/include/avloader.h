#pragma once
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void* AvLoaderHandle;

// Open a video file. Tries hardware decoders first; falls back to software.
// Returns NULL on failure.
AvLoaderHandle avloader_open(const char* path);
void           avloader_close(AvLoaderHandle h);

// Immutable properties (safe to read from any thread without locking)
int    avloader_width(AvLoaderHandle h);
int    avloader_height(AvLoaderHandle h);
int    avloader_pixel_format(AvLoaderHandle h);   // AVPixelFormat value (SW format)
// AVColorRange: 1=MPEG/limited (16-235), 2=JPEG/full (0-255), 0=unspecified (treat as limited)
int    avloader_color_range(AvLoaderHandle h);
double  avloader_native_fps(AvLoaderHandle h);
int64_t avloader_frame_count(AvLoaderHandle h); // exact frame count (always > 0 for seekable files)

// YUV plane info
int  avloader_yuv_plane_count(AvLoaderHandle h);
void avloader_yuv_plane_info(AvLoaderHandle h, int plane_idx,
                              int* tex_width, int* tex_height,
                              int* bytes_per_texel);

// Decode frame_num (1-based, relative to target_fps) into pre-allocated plane buffers.
// target_fps: frame rate used for frame-number ↔ timestamp mapping.
// bytes_per_row[i] = tex_width[i] * bytes_per_texel[i]  (compact, no padding).
// Thread-safe: serialised by an internal mutex.
// Returns 0 on success, -1 on failure.
int avloader_decode_frame(AvLoaderHandle h, uint64_t frame_num, double target_fps,
                          int num_planes, uint8_t** plane_bufs,
                          const int* bytes_per_row);

// Decode frame_num to RGB24 (channels=3) or RGBA32 (channels=4).
// target_fps: frame rate used for frame-number ↔ timestamp mapping.
// out_buf must hold at least width * height * channels bytes.
// Thread-safe: serialised by an internal mutex.
// Returns 0 on success, -1 on failure.
int avloader_decode_frame_rgb(AvLoaderHandle h, uint64_t frame_num, double target_fps,
                               uint8_t* out_buf, size_t buf_size, int channels);

#ifdef __cplusplus
}
#endif
