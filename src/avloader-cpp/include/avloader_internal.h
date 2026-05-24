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

    // Resampler: converts decoded format → AV_SAMPLE_FMT_FLTP at same sample rate
    SwrContext* swr_ctx = nullptr;

    // Working state (protected by decode_mutex)
    AVFrame*  frame  = nullptr;
    AVPacket* packet = nullptr;

    std::mutex decode_mutex;

    ~AvAudio() {
        if (swr_ctx)   swr_free(&swr_ctx);
        if (frame)     av_frame_free(&frame);
        if (packet)    av_packet_free(&packet);
        if (codec_ctx) avcodec_free_context(&codec_ctx);
        if (fmt_ctx)   avformat_close_input(&fmt_ctx);
    }
};
