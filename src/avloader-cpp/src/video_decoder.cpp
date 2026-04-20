#include "avloader.h"
#include <algorithm>
#include <cstring>
#include <string>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/imgutils.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>
#include <libswscale/swscale.h>
}

struct AvLoader {
    AVFormatContext* fmt_ctx  = nullptr;
    AVCodecContext*  codec_ctx = nullptr;
    int              video_stream_idx = -1;

    int            width      = 0;
    int            height     = 0;
    AVPixelFormat  pix_fmt    = AV_PIX_FMT_NONE;
    double         native_fps = 0.0;
    double         target_fps = 0.0;

    AVFrame*  frame  = nullptr;
    AVPacket* packet = nullptr;

    // swscale context, re-created if needed
    SwsContext* sws_rgb  = nullptr;
    SwsContext* sws_rgba = nullptr;

    ~AvLoader() {
        if (sws_rgb)   sws_freeContext(sws_rgb);
        if (sws_rgba)  sws_freeContext(sws_rgba);
        if (frame)     av_frame_free(&frame);
        if (packet)    av_packet_free(&packet);
        if (codec_ctx) avcodec_free_context(&codec_ctx);
        if (fmt_ctx)   avformat_close_input(&fmt_ctx);
    }
};

// ─── seek + decode until we reach the frame closest to target_time ──────────
static bool decode_to_time(AvLoader* ldr, double target_time) {
    AVStream* stream = ldr->fmt_ctx->streams[ldr->video_stream_idx];

    // Seek slightly before target (one native frame early as safety margin)
    double margin    = (ldr->native_fps > 0.0) ? (1.0 / ldr->native_fps) : 0.04;
    double seek_time = std::max(0.0, target_time - margin);
    int64_t seek_pts = static_cast<int64_t>(seek_time * AV_TIME_BASE);

    int ret = av_seek_frame(ldr->fmt_ctx, -1, seek_pts, AVSEEK_FLAG_BACKWARD);
    if (ret < 0) return false;
    avcodec_flush_buffers(ldr->codec_ctx);

    av_frame_unref(ldr->frame);
    bool found = false;

    while (!found) {
        ret = av_read_frame(ldr->fmt_ctx, ldr->packet);
        if (ret == AVERROR_EOF) break;
        if (ret < 0) break;

        if (ldr->packet->stream_index != ldr->video_stream_idx) {
            av_packet_unref(ldr->packet);
            continue;
        }

        ret = avcodec_send_packet(ldr->codec_ctx, ldr->packet);
        av_packet_unref(ldr->packet);
        if (ret < 0 && ret != AVERROR(EAGAIN)) break;

        while ((ret = avcodec_receive_frame(ldr->codec_ctx, ldr->frame)) >= 0) {
            int64_t pts = (ldr->frame->best_effort_timestamp != AV_NOPTS_VALUE)
                              ? ldr->frame->best_effort_timestamp
                              : ldr->frame->pts;
            double frame_time = (pts != AV_NOPTS_VALUE)
                                    ? pts * av_q2d(stream->time_base)
                                    : 0.0;

            // Accept first frame that reaches or passes target_time
            if (frame_time >= target_time - 0.5 / ldr->target_fps) {
                found = true;
                break;
            }
            av_frame_unref(ldr->frame);
        }
    }

    return found && ldr->frame->data[0] != nullptr;
}

// ─── public API ─────────────────────────────────────────────────────────────
AvLoaderHandle avloader_open(const char* path, double target_fps) {
    auto* ldr = new (std::nothrow) AvLoader();
    if (!ldr) return nullptr;

    ldr->target_fps = target_fps;

    // Open input
    if (avformat_open_input(&ldr->fmt_ctx, path, nullptr, nullptr) < 0) {
        delete ldr; return nullptr;
    }
    if (avformat_find_stream_info(ldr->fmt_ctx, nullptr) < 0) {
        delete ldr; return nullptr;
    }

    // Find best video stream
    const AVCodec* codec = nullptr;
    int idx = av_find_best_stream(ldr->fmt_ctx, AVMEDIA_TYPE_VIDEO,
                                  -1, -1, &codec, 0);
    if (idx < 0 || !codec) { delete ldr; return nullptr; }
    ldr->video_stream_idx = idx;

    AVStream* stream = ldr->fmt_ctx->streams[idx];

    // Compute native fps
    AVRational fr = stream->avg_frame_rate;
    ldr->native_fps = (fr.num > 0 && fr.den > 0)
                          ? av_q2d(fr)
                          : 30.0;

    // Open decoder
    ldr->codec_ctx = avcodec_alloc_context3(codec);
    if (!ldr->codec_ctx) { delete ldr; return nullptr; }
    avcodec_parameters_to_context(ldr->codec_ctx, stream->codecpar);
    ldr->codec_ctx->thread_count = 0;  // auto thread count
    if (avcodec_open2(ldr->codec_ctx, codec, nullptr) < 0) {
        delete ldr; return nullptr;
    }

    ldr->width   = ldr->codec_ctx->width;
    ldr->height  = ldr->codec_ctx->height;
    ldr->pix_fmt = ldr->codec_ctx->pix_fmt;

    ldr->frame  = av_frame_alloc();
    ldr->packet = av_packet_alloc();
    if (!ldr->frame || !ldr->packet) { delete ldr; return nullptr; }

    return static_cast<AvLoaderHandle>(ldr);
}

void avloader_close(AvLoaderHandle h) {
    delete static_cast<AvLoader*>(h);
}

int avloader_width(AvLoaderHandle h) {
    return static_cast<AvLoader*>(h)->width;
}
int avloader_height(AvLoaderHandle h) {
    return static_cast<AvLoader*>(h)->height;
}
int avloader_pixel_format(AvLoaderHandle h) {
    return static_cast<int>(static_cast<AvLoader*>(h)->pix_fmt);
}
double avloader_native_fps(AvLoaderHandle h) {
    return static_cast<AvLoader*>(h)->native_fps;
}

int avloader_get_frame_rgb(AvLoaderHandle h, uint64_t frame_num,
                           uint8_t* out_buf, size_t buf_size, int channels) {
    auto* ldr = static_cast<AvLoader*>(h);

    double target_time = (frame_num - 1) / ldr->target_fps;
    if (!decode_to_time(ldr, target_time)) return -1;

    AVPixelFormat dst_fmt = (channels == 4) ? AV_PIX_FMT_RGBA : AV_PIX_FMT_RGB24;
    SwsContext*&  sws_ref = (channels == 4) ? ldr->sws_rgba : ldr->sws_rgb;

    // Recreate swscale context only when source format changes
    if (!sws_ref) {
        sws_ref = sws_getContext(ldr->width, ldr->height, ldr->pix_fmt,
                                 ldr->width, ldr->height, dst_fmt,
                                 SWS_BICUBIC, nullptr, nullptr, nullptr);
        if (!sws_ref) return -1;
    }

    uint8_t* dst_data[4]    = {out_buf, nullptr, nullptr, nullptr};
    int      dst_linesize[4] = {ldr->width * channels, 0, 0, 0};

    sws_scale(sws_ref,
              ldr->frame->data, ldr->frame->linesize,
              0, ldr->height,
              dst_data, dst_linesize);
    return 0;
}

// ─── YUV plane helpers ───────────────────────────────────────────────────────

// Determine how many planes and their wgpu-compatible dimensions.
// For semi-planar (NV12/NV21) the UV plane is treated as Rg8Unorm
// (tex_width = luma_w/2, bytes_per_texel = 2).
static void compute_plane_info(const AvLoader* ldr, int plane_idx,
                               int* tex_w, int* tex_h, int* bpt) {
    const AVPixFmtDescriptor* desc = av_pix_fmt_desc_get(ldr->pix_fmt);
    if (!desc) {
        if (tex_w) *tex_w = ldr->width;
        if (tex_h) *tex_h = ldr->height;
        if (bpt)   *bpt   = 1;
        return;
    }

    int lw = ldr->width;
    int lh = ldr->height;

    if (plane_idx == 0) {
        // Luma plane: always full size, 1 byte/sample (for 8-bit)
        if (tex_w) *tex_w = lw;
        if (tex_h) *tex_h = lh;
        if (bpt)   *bpt   = 1;
        return;
    }

    // Chroma planes
    int shift_w = desc->log2_chroma_w;
    int shift_h = desc->log2_chroma_h;
    int cw = -((-lw) >> shift_w);  // ceiling division
    int ch = -((-lh) >> shift_h);

    bool is_semiplanar = (desc->flags & AV_PIX_FMT_FLAG_PLANAR) == 0 ||
                         (plane_idx == 1 && ldr->pix_fmt == AV_PIX_FMT_NV12) ||
                         (plane_idx == 1 && ldr->pix_fmt == AV_PIX_FMT_NV21) ||
                         (plane_idx == 1 && ldr->pix_fmt == AV_PIX_FMT_NV16);

    // Check actual semi-planar formats by name
    const char* name = desc->name;
    bool semiplanar = false;
    if (name) {
        std::string n(name);
        semiplanar = (n.find("nv") == 0) || (n.find("p0") == 0);
    }

    if (semiplanar && plane_idx == 1) {
        // UV interleaved: tex_width = cw, bytes_per_texel = 2 (Rg8Unorm)
        if (tex_w) *tex_w = cw;
        if (tex_h) *tex_h = ch;
        if (bpt)   *bpt   = 2;
    } else {
        // Fully planar U or V
        if (tex_w) *tex_w = cw;
        if (tex_h) *tex_h = ch;
        if (bpt)   *bpt   = 1;
    }
}

int avloader_yuv_plane_count(AvLoaderHandle h) {
    auto* ldr = static_cast<AvLoader*>(h);
    const AVPixFmtDescriptor* desc = av_pix_fmt_desc_get(ldr->pix_fmt);
    if (!desc) return 3;

    // Count non-zero planes in the frame after decoding at least one frame.
    // Use descriptor nb_components as a heuristic.
    // For semi-planar (NV12): 2 planes. For planar: 3 planes.
    int planes = 0;
    for (int i = 0; i < 4; i++) {
        // Check if this plane index is used by any component
        bool used = false;
        for (int c = 0; c < desc->nb_components; c++) {
            if (desc->comp[c].plane == i) { used = true; break; }
        }
        if (used) planes = i + 1;
    }
    return std::max(1, planes);
}

void avloader_yuv_plane_info(AvLoaderHandle h, int plane_idx,
                             int* tex_width, int* tex_height,
                             int* bytes_per_texel) {
    compute_plane_info(static_cast<AvLoader*>(h), plane_idx,
                       tex_width, tex_height, bytes_per_texel);
}

int avloader_get_frame_yuv(AvLoaderHandle h, uint64_t frame_num,
                           int num_planes,
                           uint8_t** plane_bufs,
                           const int* bytes_per_row) {
    auto* ldr = static_cast<AvLoader*>(h);

    double target_time = (frame_num - 1) / ldr->target_fps;
    if (!decode_to_time(ldr, target_time)) return -1;

    for (int p = 0; p < num_planes; p++) {
        if (!ldr->frame->data[p] || !plane_bufs[p]) continue;

        int tw, th, bpt;
        compute_plane_info(ldr, p, &tw, &th, &bpt);

        int src_stride = ldr->frame->linesize[p];
        int dst_stride = bytes_per_row[p];
        int row_bytes  = std::min(tw * bpt, std::min(src_stride, dst_stride));

        for (int row = 0; row < th; row++) {
            std::memcpy(plane_bufs[p] + row * dst_stride,
                        ldr->frame->data[p] + row * src_stride,
                        row_bytes);
        }
    }

    return 0;
}
