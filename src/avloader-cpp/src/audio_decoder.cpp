#include "avloader.h"
#include "avloader_internal.h"
#include <algorithm>
#include <cstdio>
#include <cstring>
#include <vector>

// ─── public API ─────────────────────────────────────────────────────────────

AvAudioHandle avloader_audio_open(const char* path) {
    auto* aud = new (std::nothrow) AvAudio();
    if (!aud) return nullptr;

    if (avformat_open_input(&aud->fmt_ctx, path, nullptr, nullptr) < 0) {
        delete aud; return nullptr;
    }
    if (avformat_find_stream_info(aud->fmt_ctx, nullptr) < 0) {
        delete aud; return nullptr;
    }

    const AVCodec* codec = nullptr;
    int idx = av_find_best_stream(aud->fmt_ctx, AVMEDIA_TYPE_AUDIO, -1, -1, &codec, 0);
    if (idx < 0 || !codec) { delete aud; return nullptr; }
    aud->audio_stream_idx = idx;

    AVStream* stream = aud->fmt_ctx->streams[idx];

    if (stream->start_time != AV_NOPTS_VALUE)
        aud->start_time = stream->start_time * av_q2d(stream->time_base);
    else if (aud->fmt_ctx->start_time != AV_NOPTS_VALUE)
        aud->start_time = aud->fmt_ctx->start_time / static_cast<double>(AV_TIME_BASE);

    if (stream->duration != AV_NOPTS_VALUE && stream->duration > 0)
        aud->duration = stream->duration * av_q2d(stream->time_base);
    else if (aud->fmt_ctx->duration != AV_NOPTS_VALUE && aud->fmt_ctx->duration > 0)
        aud->duration = aud->fmt_ctx->duration / static_cast<double>(AV_TIME_BASE);

    aud->codec_ctx = avcodec_alloc_context3(codec);
    if (!aud->codec_ctx) { delete aud; return nullptr; }
    avcodec_parameters_to_context(aud->codec_ctx, stream->codecpar);
    aud->codec_ctx->thread_count = 0;
    if (avcodec_open2(aud->codec_ctx, codec, nullptr) < 0) {
        delete aud; return nullptr;
    }

    aud->channels    = aud->codec_ctx->ch_layout.nb_channels;
    aud->sample_rate = aud->codec_ctx->sample_rate;
    aud->bitrate     = aud->codec_ctx->bit_rate;
    if (aud->bitrate == 0 && stream->codecpar->bit_rate > 0)
        aud->bitrate = stream->codecpar->bit_rate;
    if (aud->bitrate == 0 && aud->fmt_ctx->bit_rate > 0)
        aud->bitrate = aud->fmt_ctx->bit_rate;

    // Initialise swresample: convert decoded format → float32 planar (FLTP)
    // at the same sample rate (no SRC, so no internal delay in swr_convert).
    if (swr_alloc_set_opts2(&aud->swr_ctx,
            &aud->codec_ctx->ch_layout, AV_SAMPLE_FMT_FLTP, aud->sample_rate,
            &aud->codec_ctx->ch_layout, aud->codec_ctx->sample_fmt, aud->sample_rate,
            0, nullptr) < 0 || swr_init(aud->swr_ctx) < 0) {
        delete aud; return nullptr;
    }

    aud->frame  = av_frame_alloc();
    aud->packet = av_packet_alloc();
    if (!aud->frame || !aud->packet) { delete aud; return nullptr; }

    printf("[avaudio] decoder: %s  ch=%d  rate=%d\n",
           codec->name, aud->channels, aud->sample_rate);
    return static_cast<AvAudioHandle>(aud);
}

void avloader_audio_close(AvAudioHandle h) {
    delete static_cast<AvAudio*>(h);
}

int     avloader_audio_channels(AvAudioHandle h)     { return static_cast<AvAudio*>(h)->channels; }
double  avloader_audio_duration(AvAudioHandle h)     { return static_cast<AvAudio*>(h)->duration; }
int64_t avloader_audio_bitrate(AvAudioHandle h)      { return static_cast<AvAudio*>(h)->bitrate; }
int     avloader_audio_sampling_rate(AvAudioHandle h){ return static_cast<AvAudio*>(h)->sample_rate; }

int64_t avloader_audio_get_audio(AvAudioHandle h, double time, double duration,
                           float* out_buf, int64_t buf_samples_per_channel) {
    auto* aud = static_cast<AvAudio*>(h);
    std::lock_guard<std::mutex> lock(aud->decode_mutex);

    AVStream* stream  = aud->fmt_ctx->streams[aud->audio_stream_idx];
    const double end_time = time + duration;
    const int channels    = aud->channels;

    // Seek slightly before target to ensure we don't miss the first frame.
    double seek_time = std::max(aud->start_time, time - 0.5);
    int64_t seek_pts = static_cast<int64_t>(
        (seek_time - aud->start_time) / av_q2d(stream->time_base));
    if (stream->start_time != AV_NOPTS_VALUE)
        seek_pts += stream->start_time;

    if (av_seek_frame(aud->fmt_ctx, aud->audio_stream_idx, seek_pts, AVSEEK_FLAG_BACKWARD) < 0) {
        if (av_seek_frame(aud->fmt_ctx, -1,
                static_cast<int64_t>(seek_time * AV_TIME_BASE),
                AVSEEK_FLAG_BACKWARD) < 0) {
            return -1;
        }
    }
    avcodec_flush_buffers(aud->codec_ctx);
    // Flush swr internal state (safe even with no-SRC context).
    swr_convert(aud->swr_ctx, nullptr, 0, nullptr, 0);

    // Temporary per-frame conversion buffer.
    // AAC=1024, MP3=1152, Opus≤2880, FLAC/PCM≤65536 samples/frame.
    const int kMaxFrameSamples = 65536;
    std::vector<std::vector<float>> tmp(channels, std::vector<float>(kMaxFrameSamples));
    std::vector<uint8_t*> tmp_ptrs(channels);
    for (int c = 0; c < channels; c++)
        tmp_ptrs[c] = reinterpret_cast<uint8_t*>(tmp[c].data());

    int64_t collected = 0;
    bool    done      = false;

    while (!done) {
        int ret = av_read_frame(aud->fmt_ctx, aud->packet);
        if (ret == AVERROR_EOF || ret < 0) break;

        if (aud->packet->stream_index != aud->audio_stream_idx) {
            av_packet_unref(aud->packet);
            continue;
        }

        ret = avcodec_send_packet(aud->codec_ctx, aud->packet);
        av_packet_unref(aud->packet);
        if (ret < 0 && ret != AVERROR(EAGAIN)) break;

        while (!done) {
            ret = avcodec_receive_frame(aud->codec_ctx, aud->frame);
            if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
            if (ret < 0) { done = true; break; }

            int64_t pts = (aud->frame->best_effort_timestamp != AV_NOPTS_VALUE)
                          ? aud->frame->best_effort_timestamp : aud->frame->pts;
            double frame_time = (pts != AV_NOPTS_VALUE)
                                ? pts * av_q2d(stream->time_base)
                                : aud->start_time;
            int nb = aud->frame->nb_samples;
            double frame_end = frame_time + static_cast<double>(nb) / aud->sample_rate;

            // Skip frames entirely before target time.
            if (frame_end <= time) {
                av_frame_unref(aud->frame);
                continue;
            }
            // Stop once we've passed the requested window.
            if (frame_time >= end_time) {
                av_frame_unref(aud->frame);
                done = true;
                break;
            }

            // Convert to AV_SAMPLE_FMT_FLTP (no-op if already FLTP).
            int converted = swr_convert(aud->swr_ctx,
                tmp_ptrs.data(), kMaxFrameSamples,
                const_cast<const uint8_t**>(aud->frame->data), nb);
            av_frame_unref(aud->frame);
            if (converted <= 0) continue;

            // How many samples at the start of this frame to skip
            // (seek overshoot: frame started before our target time).
            int skip = 0;
            if (frame_time < time) {
                double skip_secs = time - frame_time;
                skip = std::min(static_cast<int>(skip_secs * aud->sample_rate), converted);
            }

            int use = std::min(converted - skip,
                               static_cast<int>(buf_samples_per_channel - collected));
            if (use <= 0) {
                if (collected >= buf_samples_per_channel) done = true;
                continue;
            }

            for (int c = 0; c < channels; c++) {
                std::memcpy(out_buf + static_cast<int64_t>(c) * buf_samples_per_channel + collected,
                            tmp[c].data() + skip,
                            static_cast<size_t>(use) * sizeof(float));
            }
            collected += use;
            if (collected >= buf_samples_per_channel) done = true;
        }
    }

    return collected;
}
