#pragma once
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void* AvLoaderHandle;

// Open a video file. Returns NULL on failure.
// target_fps: desired frame rate for frame-number mapping (does not re-encode).
AvLoaderHandle avloader_open(const char* path, double target_fps);
void avloader_close(AvLoaderHandle h);

// Video dimensions (pixels)
int avloader_width(AvLoaderHandle h);
int avloader_height(AvLoaderHandle h);

// Native AVPixelFormat value of the decoded stream
int avloader_pixel_format(AvLoaderHandle h);

// Actual video FPS from the stream (not target_fps)
double avloader_native_fps(AvLoaderHandle h);

// Fill a pre-allocated RGB/RGBA buffer for the given 1-based frame number
// (frame timing is mapped via target_fps set at open time).
// channels: 3 for RGB24, 4 for RGBA32
// out_buf must have at least width * height * channels bytes.
// Returns 0 on success, -1 on failure.
int avloader_get_frame_rgb(AvLoaderHandle h, uint64_t frame_num,
                           uint8_t* out_buf, size_t buf_size, int channels);

// Number of native YUV planes (determined by pixel format)
int avloader_yuv_plane_count(AvLoaderHandle h);

// Texture dimensions and bytes-per-texel for one plane.
// tex_width / tex_height are in texels (wgpu texture dimensions).
// bytes_per_texel is 1 for R8Unorm planes, 2 for Rg8Unorm planes (e.g. NV12 UV).
void avloader_yuv_plane_info(AvLoaderHandle h, int plane_idx,
                             int* tex_width, int* tex_height,
                             int* bytes_per_texel);

// Fill pre-allocated YUV plane buffers (original format, no conversion).
// plane_bufs[i] must be allocated with tex_width[i] * tex_height[i] * bytes_per_texel[i] bytes.
// bytes_per_row[i] must equal tex_width[i] * bytes_per_texel[i] (compact, no padding).
// Returns 0 on success, -1 on failure.
int avloader_get_frame_yuv(AvLoaderHandle h, uint64_t frame_num,
                           int num_planes,
                           uint8_t** plane_bufs,
                           const int* bytes_per_row);

#ifdef __cplusplus
}
#endif
