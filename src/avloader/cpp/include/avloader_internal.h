#pragma once
#include <mutex>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/imgutils.h>
#include <libavutil/hwcontext.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>
#include <libswscale/swscale.h>
#include <libswresample/swresample.h>
}

// ─── AvLoader (video) ────────────────────────────────────────────────────────

struct AvLoader {
    AVFormatContext* fmt_ctx          = nullptr;
    AVCodecContext*  codec_ctx        = nullptr;
    int              video_stream_idx = -1;

    // Immutable after open (no lock needed)
    int           width       = 0;
    int           height      = 0;
    AVPixelFormat pix_fmt     = AV_PIX_FMT_NONE;
    AVColorRange  color_range = AVCOL_RANGE_UNSPECIFIED;
    double        native_fps       = 0.0;
    double        duration         = 0.0;
    double        start_time       = 0.0;
    int64_t       probed_nb_frames = 0;

    // Set at open time when `pix_fmt` is a single-plane/packed format (e.g.
    // GRAY8, PAL8, RGB24, RGBA — common for still-image codecs like PNG/BMP/GIF)
    // that the GPU texture pipeline's plane classification (`yuv_layout_from_pix_fmt`
    // on the Rust side) cannot represent: it only recognises 2-plane semi-planar
    // and 4-plane alpha-YUV formats explicitly, and otherwise assumes a genuine
    // 3-plane YUV layout. Every decoded frame is transparently converted (via
    // `sws_normalize`) from `pix_fmt` into `normalized_pix_fmt` (YUV420P/YUVA420P)
    // before any caller sees it, so `pix_fmt` keeps meaning "what the codec
    // actually outputs" while all plane/format queries report the converted,
    // pipeline-safe format instead (see `effective_pix_fmt()`).
    bool          needs_pix_normalize = false;
    AVPixelFormat normalized_pix_fmt  = AV_PIX_FMT_NONE;
    SwsContext*   sws_normalize       = nullptr;

    // Hardware decoding
    AVBufferRef*  hw_device_ctx = nullptr;
    AVPixelFormat hw_pix_fmt    = AV_PIX_FMT_NONE;
    AVFrame*      hw_frame      = nullptr;

    // Decoder working state (protected by decode_mutex)
    AVFrame*  frame      = nullptr;
    AVPacket* packet     = nullptr;
    double    last_decoded_time = -1.0;
    double    last_target_time  = -1.0;
    double    last_target_fps   = 0.0;

    // sws contexts for RGB conversion (protected by decode_mutex)
    SwsContext* sws_rgb  = nullptr;
    SwsContext* sws_rgba = nullptr;

    std::mutex decode_mutex;

    ~AvLoader() {
        if (sws_rgb)       sws_freeContext(sws_rgb);
        if (sws_rgba)      sws_freeContext(sws_rgba);
        if (sws_normalize) sws_freeContext(sws_normalize);
        if (hw_frame)      av_frame_free(&hw_frame);
        if (frame)         av_frame_free(&frame);
        if (packet)        av_packet_free(&packet);
        if (codec_ctx)     avcodec_free_context(&codec_ctx);
        if (hw_device_ctx) av_buffer_unref(&hw_device_ctx);
        if (fmt_ctx)       avformat_close_input(&fmt_ctx);
    }
};

// ─── AvAudio ─────────────────────────────────────────────────────────────────

struct AvAudio {
    AVFormatContext* fmt_ctx          = nullptr;
    AVCodecContext*  codec_ctx        = nullptr;
    int              audio_stream_idx = -1;

    // Immutable after open
    int     channels    = 0;
    double  duration    = 0.0;
    int     bit_depth   = 0;
    int     sample_rate = 0;
    double  start_time  = 0.0;

    // Working state (protected by decode_mutex)
    AVFrame*  frame  = nullptr;
    AVPacket* packet = nullptr;

    std::mutex decode_mutex;

    ~AvAudio() {
        if (frame)     av_frame_free(&frame);
        if (packet)    av_packet_free(&packet);
        if (codec_ctx) avcodec_free_context(&codec_ctx);
        if (fmt_ctx)   avformat_close_input(&fmt_ctx);
    }
};
