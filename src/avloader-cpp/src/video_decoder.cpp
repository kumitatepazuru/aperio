#include "avloader.h"
#include "avloader_internal.h"
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>

static constexpr uint64_t PREFETCH_RESET_THRESH = 60;

// ─── Hardware decoder priority ───────────────────────────────────────────────
// Tried in order; first one that initialises successfully wins.
static const AVHWDeviceType kHwPriority[] = {
    AV_HWDEVICE_TYPE_D3D11VA,
    AV_HWDEVICE_TYPE_CUDA,
    AV_HWDEVICE_TYPE_DXVA2,
    AV_HWDEVICE_TYPE_VAAPI,
    AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
    AV_HWDEVICE_TYPE_NONE,  // sentinel
};

// ─── plane helpers ───────────────────────────────────────────────────────────

static void compute_plane_info(const AvLoader* ldr, int plane_idx,
                               int* tex_w, int* tex_h, int* bpt) {
    const AVPixFmtDescriptor* desc = av_pix_fmt_desc_get(ldr->pix_fmt);
    if (plane_idx == 0 || !desc) {
        if (tex_w) *tex_w = ldr->width;
        if (tex_h) *tex_h = ldr->height;
        // comp[0].step gives bytes per luma texel (1 for 8-bit, 2 for 10/12/16-bit)
        if (bpt)   *bpt   = (desc && desc->nb_components > 0) ? desc->comp[0].step : 1;
        return;
    }

    int shift_w = desc->log2_chroma_w;
    int shift_h = desc->log2_chroma_h;
    int cw      = -((-ldr->width)  >> shift_w);
    int ch      = -((-ldr->height) >> shift_h);

    if (tex_w) *tex_w = cw;
    if (tex_h) *tex_h = ch;
    if (bpt) {
        // comp[ci].step: for semi-planar UV this equals U_step which spans one full UV pair
        // (e.g. NV12→2, P010→4); for planar chroma it equals one sample's byte width.
        int ci = std::min(plane_idx, desc->nb_components - 1);
        *bpt = desc->comp[ci].step;
    }
}

static int plane_count_for(const AvLoader* ldr) {
    const AVPixFmtDescriptor* desc = av_pix_fmt_desc_get(ldr->pix_fmt);
    if (!desc) return 3;
    int planes = 0;
    for (int i = 0; i < 4; i++) {
        for (int c = 0; c < desc->nb_components; c++) {
            if (desc->comp[c].plane == i) { planes = i + 1; break; }
        }
    }
    return std::max(1, planes);
}

// ─── HW helpers ──────────────────────────────────────────────────────────────

// Returns the HW pixel format the codec exposes for the given device type,
// or AV_PIX_FMT_NONE if unsupported.
static AVPixelFormat codec_hw_pix_fmt(const AVCodec* codec, AVHWDeviceType type) {
    for (int i = 0; ; i++) {
        const AVCodecHWConfig* cfg = avcodec_get_hw_config(codec, i);
        if (!cfg) return AV_PIX_FMT_NONE;
        if ((cfg->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX)
            && cfg->device_type == type)
            return cfg->pix_fmt;
    }
}

// Seek to the last keyframe, scan remaining video packets for the maximum PTS,
// and derive the exact frame count.  Resets the demuxer to the stream start
// afterwards so subsequent decode calls begin from a clean state.
// Called only for containers that do not store nb_frames (e.g. MKV/WebM).
static void probe_end_pts(AvLoader* ldr) {
    AVStream* stream = ldr->fmt_ctx->streams[ldr->video_stream_idx];

    // Seek to the last keyframe.  Use the container duration as the target so
    // the seek lands as close to the end as possible before reading forward.
    int64_t seek_ts = (ldr->fmt_ctx->duration != AV_NOPTS_VALUE && ldr->fmt_ctx->duration > 0)
                      ? av_rescale_q(ldr->fmt_ctx->duration, AV_TIME_BASE_Q, stream->time_base)
                      : INT64_MAX;
    if (av_seek_frame(ldr->fmt_ctx, ldr->video_stream_idx,
                      seek_ts, AVSEEK_FLAG_BACKWARD) < 0) {
        // Non-seekable stream – skip probe entirely.
        return;
    }

    // Scan every remaining packet to find the maximum video PTS (display order).
    // For B-frame content the packets near EOF are not in display order, so we
    // must take the max rather than stopping at the first video packet.
    AVPacket* pkt = av_packet_alloc();
    if (!pkt) return;

    int64_t max_pts = AV_NOPTS_VALUE;
    while (av_read_frame(ldr->fmt_ctx, pkt) >= 0) {
        if (pkt->stream_index == ldr->video_stream_idx) {
            int64_t pts = (pkt->pts != AV_NOPTS_VALUE) ? pkt->pts : pkt->dts;
            if (pts != AV_NOPTS_VALUE && (max_pts == AV_NOPTS_VALUE || pts > max_pts))
                max_pts = pts;
        }
        av_packet_unref(pkt);
    }
    av_packet_free(&pkt);

    if (max_pts != AV_NOPTS_VALUE && ldr->native_fps > 0.0) {
        double last_frame_time = max_pts * av_q2d(stream->time_base);
        double content_duration = last_frame_time - ldr->start_time;
        // +1: the frame at last_frame_time itself is included in the count.
        ldr->probed_nb_frames =
            static_cast<int64_t>(std::round(content_duration * ldr->native_fps)) + 1;
        // Also refine duration so the duration-based fallback in Python is accurate.
        ldr->duration = content_duration + 1.0 / ldr->native_fps;
    }

    // Reset demuxer to stream start for subsequent decode calls.
    int64_t seek_to = (stream->start_time != AV_NOPTS_VALUE) ? stream->start_time : 0;
    av_seek_frame(ldr->fmt_ctx, ldr->video_stream_idx, seek_to, AVSEEK_FLAG_BACKWARD);
    avcodec_flush_buffers(ldr->codec_ctx);
    av_frame_unref(ldr->frame);
}

// Decode one frame to discover the actual SW pixel format produced by the HW
// decoder, then reset state so callers start from a clean position.
// Called only from avloader_video_open before the handle is shared.
static void probe_hw_pixel_format(AvLoader* ldr) {
    AVPacket* pkt = av_packet_alloc();
    if (!pkt) return;

    AVFrame* sw = av_frame_alloc();
    if (!sw) { av_packet_free(&pkt); return; }

    bool found = false;
    while (!found && av_read_frame(ldr->fmt_ctx, pkt) >= 0) {
        if (pkt->stream_index != ldr->video_stream_idx) {
            av_packet_unref(pkt);
            continue;
        }
        if (avcodec_send_packet(ldr->codec_ctx, pkt) >= 0) {
            while (avcodec_receive_frame(ldr->codec_ctx, ldr->frame) >= 0) {
                if (ldr->frame->format == (int)ldr->hw_pix_fmt
                        && av_hwframe_transfer_data(sw, ldr->frame, 0) >= 0) {
                    ldr->pix_fmt = static_cast<AVPixelFormat>(sw->format);
                    found = true;
                }
                av_frame_unref(ldr->frame);
                if (found) break;
            }
        }
        av_packet_unref(pkt);
    }

    av_frame_free(&sw);
    av_packet_free(&pkt);

    // Reset to stream start so the first real decode call begins cleanly.
    AVStream* stream = ldr->fmt_ctx->streams[ldr->video_stream_idx];
    int64_t seek_to = (stream->start_time != AV_NOPTS_VALUE) ? stream->start_time : 0;
    av_seek_frame(ldr->fmt_ctx, ldr->video_stream_idx, seek_to, AVSEEK_FLAG_BACKWARD);
    avcodec_flush_buffers(ldr->codec_ctx);
    av_frame_unref(ldr->frame);
}

// ─── core decode (must be called with decode_mutex held) ─────────────────────

static bool decode_to_time(AvLoader* ldr, double target_time, double target_fps) {
    AVStream* stream = ldr->fmt_ctx->streams[ldr->video_stream_idx];
    const double tol = (target_fps > 0.0) ? 0.5 / target_fps : 0.02;

    // Seek when: first call, going backward, or jumping further ahead than the
    // prefetch-reset window (keeps the threshold consistent with the Rust cache).
    const double seek_fwd_limit = (target_fps > 0.0)
                                    ? PREFETCH_RESET_THRESH / target_fps : 2.0;

    bool going_backward = (ldr->last_target_time >= 0.0) && (target_time < ldr->last_target_time - 0.001);
    const bool need_seek = ldr->last_decoded_time < 0.0
                        || going_backward
                        || (target_time - ldr->last_decoded_time) > seek_fwd_limit;

    if (need_seek) {
        double margin    = (ldr->native_fps > 0.0) ? (1.0 / ldr->native_fps) : 0.04;
        // Never seek before stream start
        double seek_time = std::max(ldr->start_time, target_time - margin);

        int64_t seek_pts = static_cast<int64_t>((seek_time - ldr->start_time) / av_q2d(stream->time_base));
        if (stream->start_time != AV_NOPTS_VALUE) {
            seek_pts += stream->start_time;
        }

        if (av_seek_frame(ldr->fmt_ctx, ldr->video_stream_idx, seek_pts, AVSEEK_FLAG_BACKWARD) < 0) {
            if (av_seek_frame(ldr->fmt_ctx, -1, static_cast<int64_t>(seek_time * AV_TIME_BASE), AVSEEK_FLAG_BACKWARD) < 0)
                return false;
        }
        avcodec_flush_buffers(ldr->codec_ctx);

        ldr->last_decoded_time = -1.0;
    } else if (ldr->frame->data[0] != nullptr && ldr->last_decoded_time >= target_time - tol) {
        // [FAST PATH]
        ldr->last_target_time = target_time;
        return true;
    }

    av_frame_unref(ldr->frame);
    bool found = false;

    // Lenient tolerance for EOF: accept the last native frame even when
    // target_time falls up to one native-frame duration past its PTS.
    // This occurs whenever target_fps > native_fps and the final project frame
    // maps to a timestamp slightly beyond the last encoded frame.
    const double eof_tol = (ldr->native_fps > 0.0)
                           ? std::max(tol, 1.0 / ldr->native_fps)
                           : tol;

    // EOF fallback: the most recent frame within eof_tol (but outside tol).
    // Kept alive across multiple recv_frames() calls so it is still available
    // even if it was discarded (unref'd) before we reached the flush step.
    AVFrame* candidate    = av_frame_alloc();
    double   candidate_ft = -1.0;

    // Helper: HW→SW transfer into ldr->frame. Returns false and unref's on error.
    auto hw_transfer = [&]() -> bool {
        if (ldr->hw_pix_fmt == AV_PIX_FMT_NONE
            || ldr->frame->format != (int)ldr->hw_pix_fmt)
            return true;
        av_frame_unref(ldr->hw_frame);
        if (av_hwframe_transfer_data(ldr->hw_frame, ldr->frame, 0) < 0) {
            av_frame_unref(ldr->frame);
            return false;
        }
        av_frame_unref(ldr->frame);
        av_frame_move_ref(ldr->frame, ldr->hw_frame);
        ldr->pix_fmt = static_cast<AVPixelFormat>(ldr->frame->format);
        return true;
    };

    // Receive frames from the decoder pipeline, transferring HW frames to CPU
    // and checking whether we have reached the desired timestamp.
    auto recv_frames = [&]() -> bool {
        int ret;
        while ((ret = avcodec_receive_frame(ldr->codec_ctx, ldr->frame)) >= 0) {
            int64_t pts = (ldr->frame->best_effort_timestamp != AV_NOPTS_VALUE)
                              ? ldr->frame->best_effort_timestamp
                              : ldr->frame->pts;
            double ft = (pts != AV_NOPTS_VALUE)
                            ? pts * av_q2d(stream->time_base)
                            : ldr->start_time;

            if (ft >= target_time - tol) {
                // Exact match: HW transfer and return.
                if (!hw_transfer()) continue;
                if (candidate) av_frame_unref(candidate);
                ldr->last_decoded_time = ft;
                ldr->last_target_time  = target_time;
                return true;
            }

            if (candidate && ft >= target_time - eof_tol && ft > candidate_ft) {
                // Within eof_tol: save as EOF fallback (with HW transfer so
                // the SW frame survives the av_frame_move_ref below).
                if (!hw_transfer()) continue;
                av_frame_move_ref(candidate, ldr->frame);  // unref old candidate
                candidate_ft = ft;
            } else {
                av_frame_unref(ldr->frame);
            }
        }
        return false;
    };

    // Drain buffered decoder output first (avoids an unnecessary packet read on
    // sequential access, which is the common path for the prefetch thread).
    if (!need_seek && recv_frames()) {
        found = true;
    }

    while (!found) {
        int ret = av_read_frame(ldr->fmt_ctx, ldr->packet);
        if (ret == AVERROR_EOF || ret < 0) break;

        if (ldr->packet->stream_index != ldr->video_stream_idx) {
            av_packet_unref(ldr->packet);
            continue;
        }

        // EAGAIN means "drain first, then retry the same packet"
        for (;;) {
            ret = avcodec_send_packet(ldr->codec_ctx, ldr->packet);
            if (ret != AVERROR(EAGAIN)) break;
            if (recv_frames()) { found = true; break; }
        }
        av_packet_unref(ldr->packet);
        if (found) break;
        if (ret < 0) break;

        if (recv_frames()) {
            found = true;
        }
    }

    // Flush decoder pipeline (drains B-frames still buffered in the decoder).
    if (!found) {
        avcodec_send_packet(ldr->codec_ctx, nullptr);
        if (recv_frames()) found = true;
    }

    // Use EOF candidate if no exact match was found.
    if (!found && candidate && candidate->data[0] != nullptr) {
        av_frame_unref(ldr->frame);
        av_frame_move_ref(ldr->frame, candidate);
        ldr->last_decoded_time = candidate_ft;
        ldr->last_target_time  = target_time;
        found = true;
    }

    if (candidate) {
        av_frame_unref(candidate);
        av_frame_free(&candidate);
    }

    if (!found) {
        printf("[avloader] DECODE failed  target=%.3fs\n", target_time);
        // Reset so the next sequential frame triggers a fresh seek+flush instead
        // of reusing the now-exhausted (EOF) decoder state, which would cause a
        // cascade of silent failures for every subsequent frame.
        ldr->last_decoded_time = -1.0;
    }

    return found && ldr->frame->data[0] != nullptr;
}

// ─── public API ─────────────────────────────────────────────────────────────

AvLoaderHandle avloader_video_open(const char* path) {
    auto* ldr = new (std::nothrow) AvLoader();
    if (!ldr) return nullptr;

    if (avformat_open_input(&ldr->fmt_ctx, path, nullptr, nullptr) < 0) {
        delete ldr; return nullptr;
    }
    if (avformat_find_stream_info(ldr->fmt_ctx, nullptr) < 0) {
        delete ldr; return nullptr;
    }

    const AVCodec* codec = nullptr;
    int idx = av_find_best_stream(ldr->fmt_ctx, AVMEDIA_TYPE_VIDEO, -1, -1, &codec, 0);
    if (idx < 0 || !codec) { delete ldr; return nullptr; }
    ldr->video_stream_idx = idx;

    AVStream* stream = ldr->fmt_ctx->streams[idx];
    AVRational fr    = stream->avg_frame_rate;
    ldr->native_fps  = (fr.num > 0 && fr.den > 0) ? av_q2d(fr) : 30.0;

    // Determine stream start time (absolute base for all timestamp arithmetic)
    if (stream->start_time != AV_NOPTS_VALUE)
        ldr->start_time = stream->start_time * av_q2d(stream->time_base);
    else if (ldr->fmt_ctx->start_time != AV_NOPTS_VALUE)
        ldr->start_time = ldr->fmt_ctx->start_time / static_cast<double>(AV_TIME_BASE);

    // Duration: prefer video-stream duration over container duration.
    // fmt_ctx->duration covers all tracks and can be inflated by a longer audio
    // track, which causes max_frame to be over-counted.
    // stream->duration is specific to the video track and avoids this problem.
    if (stream->duration != AV_NOPTS_VALUE && stream->duration > 0)
        ldr->duration = stream->duration * av_q2d(stream->time_base);
    else if (ldr->fmt_ctx->duration != AV_NOPTS_VALUE && ldr->fmt_ctx->duration > 0)
        ldr->duration = ldr->fmt_ctx->duration / static_cast<double>(AV_TIME_BASE);

    // ── Try hardware decoders in priority order ───────────────────────────
    bool hw_opened = false;
    for (int hi = 0; kHwPriority[hi] != AV_HWDEVICE_TYPE_NONE && !hw_opened; hi++) {
        AVHWDeviceType hw_type = kHwPriority[hi];

        AVPixelFormat hw_fmt = codec_hw_pix_fmt(codec, hw_type);
        if (hw_fmt == AV_PIX_FMT_NONE) continue;

        AVBufferRef* hw_ctx = nullptr;
        if (av_hwdevice_ctx_create(&hw_ctx, hw_type, nullptr, nullptr, 0) < 0) continue;

        AVCodecContext* ctx = avcodec_alloc_context3(codec);
        if (!ctx) { av_buffer_unref(&hw_ctx); continue; }

        avcodec_parameters_to_context(ctx, stream->codecpar);
        ctx->hw_device_ctx = av_buffer_ref(hw_ctx);
        ctx->thread_count  = 0;

        if (avcodec_open2(ctx, codec, nullptr) >= 0) {
            ldr->codec_ctx     = ctx;
            ldr->hw_device_ctx = hw_ctx;
            ldr->hw_pix_fmt    = hw_fmt;
            hw_opened = true;
            printf("[avloader] HW decoder: %s\n", av_hwdevice_get_type_name(hw_type));
        } else {
            avcodec_free_context(&ctx);
            av_buffer_unref(&hw_ctx);
        }
    }

    // ── Software fallback ─────────────────────────────────────────────────
    if (!hw_opened) {
        ldr->codec_ctx = avcodec_alloc_context3(codec);
        if (!ldr->codec_ctx) { delete ldr; return nullptr; }
        avcodec_parameters_to_context(ldr->codec_ctx, stream->codecpar);
        ldr->codec_ctx->thread_count = 0;
        if (avcodec_open2(ldr->codec_ctx, codec, nullptr) < 0) {
            delete ldr; return nullptr;
        }
        printf("[avloader] SW decoder: %s\n", codec->name);
    }

    ldr->frame  = av_frame_alloc();
    ldr->packet = av_packet_alloc();
    if (!ldr->frame || !ldr->packet) { delete ldr; return nullptr; }

    if (hw_opened) {
        ldr->hw_frame = av_frame_alloc();
        if (!ldr->hw_frame) { delete ldr; return nullptr; }

        ldr->pix_fmt = AV_PIX_FMT_NV12;  // tentative; corrected by probe below
        probe_hw_pixel_format(ldr);
    } else {
        ldr->pix_fmt = ldr->codec_ctx->pix_fmt;
    }

    ldr->width  = ldr->codec_ctx->width;
    ldr->height = ldr->codec_ctx->height;

    // Probe gives the most accurate color_range for HW decoders (SEI/VUI parsed);
    // fall back to container metadata when unspecified.
    ldr->color_range = ldr->codec_ctx->color_range;
    if (ldr->color_range == AVCOL_RANGE_UNSPECIFIED) {
        ldr->color_range =
            ldr->fmt_ctx->streams[ldr->video_stream_idx]->codecpar->color_range;
    }

    // For containers that don't store nb_frames (MKV, WebM, MPEG-TS, …),
    // scan to the end of the file to find the exact last-frame PTS.
    // This is skipped when the container already provides a reliable count.
    if (ldr->fmt_ctx->streams[ldr->video_stream_idx]->nb_frames == 0)
        probe_end_pts(ldr);

    return static_cast<AvLoaderHandle>(ldr);
}

void avloader_video_close(AvLoaderHandle h) {
    delete static_cast<AvLoader*>(h);
}

int    avloader_video_width(AvLoaderHandle h)        { return static_cast<AvLoader*>(h)->width; }
int    avloader_video_height(AvLoaderHandle h)       { return static_cast<AvLoader*>(h)->height; }
int    avloader_video_pixel_format(AvLoaderHandle h) { return static_cast<int>(static_cast<AvLoader*>(h)->pix_fmt); }
int    avloader_video_color_range(AvLoaderHandle h)  { return static_cast<int>(static_cast<AvLoader*>(h)->color_range); }
double  avloader_video_native_fps(AvLoaderHandle h)  { return static_cast<AvLoader*>(h)->native_fps; }
int64_t avloader_video_frame_count(AvLoaderHandle h) {
    auto* ldr = static_cast<AvLoader*>(h);
    int64_t n = ldr->fmt_ctx->streams[ldr->video_stream_idx]->nb_frames;
    return (n > 0) ? n : ldr->probed_nb_frames;
}

int avloader_video_yuv_plane_count(AvLoaderHandle h) {
    return plane_count_for(static_cast<AvLoader*>(h));
}

void avloader_video_yuv_plane_info(AvLoaderHandle h, int plane_idx,
                                    int* tex_width, int* tex_height, int* bytes_per_texel) {
    compute_plane_info(static_cast<AvLoader*>(h), plane_idx,
                       tex_width, tex_height, bytes_per_texel);
}

int avloader_video_decode_frame(AvLoaderHandle h, uint64_t frame_num, double target_fps,
                                 int num_planes, uint8_t** plane_bufs,
                                 const int* bytes_per_row) {
    auto* ldr = static_cast<AvLoader*>(h);
    std::lock_guard<std::mutex> lock(ldr->decode_mutex);

    // target_time is absolute (includes stream start_time), matching the actual
    // pts values in the stream. Without start_time, every frame would appear to
    // go backward relative to last_decoded_time and trigger a seek.
    double target_time = ldr->start_time + (frame_num - 1.0) / target_fps;

    if (!decode_to_time(ldr, target_time, target_fps)) return -1;

    int np = std::min(plane_count_for(ldr), num_planes);
    for (int p = 0; p < np; p++) {
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

int avloader_video_decode_frame_rgb(AvLoaderHandle h, uint64_t frame_num, double target_fps,
                                     uint8_t* out_buf, size_t /*buf_size*/, int channels) {
    auto* ldr = static_cast<AvLoader*>(h);
    std::lock_guard<std::mutex> lock(ldr->decode_mutex);

    double target_time = ldr->start_time + (frame_num - 1.0) / target_fps;
    if (!decode_to_time(ldr, target_time, target_fps)) return -1;

    AVPixelFormat src_fmt = static_cast<AVPixelFormat>(ldr->frame->format);
    AVPixelFormat dst_fmt = (channels == 4) ? AV_PIX_FMT_RGBA64LE : AV_PIX_FMT_RGB48LE;
    SwsContext*&  sws_ref = (channels == 4) ? ldr->sws_rgba : ldr->sws_rgb;

    if (sws_ref && src_fmt != ldr->pix_fmt) {
        sws_freeContext(sws_ref);
        sws_ref = nullptr;
    }
    if (!sws_ref) {
        sws_ref = sws_getContext(ldr->width, ldr->height, src_fmt,
                                 ldr->width, ldr->height, dst_fmt,
                                 SWS_BICUBIC, nullptr, nullptr, nullptr);
        if (!sws_ref) return -1;

        // Explicitly match the GPU shader path (BT.709, source's actual
        // limited/full range) instead of relying on swscale's defaults.
        const int* coeff = sws_getCoefficients(SWS_CS_ITU709);
        int src_range = (ldr->color_range == AVCOL_RANGE_JPEG) ? 1 : 0;
        sws_setColorspaceDetails(sws_ref, coeff, src_range, coeff, /*dstRange=*/1,
                                 0, 1 << 16, 1 << 16);
        ldr->pix_fmt = src_fmt;
    }

    uint8_t* dst_data[4]     = { out_buf, nullptr, nullptr, nullptr };
    int      dst_linesize[4] = { ldr->width * channels * 2, 0, 0, 0 };

    sws_scale(sws_ref,
              const_cast<const uint8_t**>(ldr->frame->data), ldr->frame->linesize,
              0, ldr->height, dst_data, dst_linesize);
    return 0;
}
