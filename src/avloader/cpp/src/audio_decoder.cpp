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

    aud->codec_ctx = avcodec_alloc_context3(codec);
    if (!aud->codec_ctx) { delete aud; return nullptr; }
    avcodec_parameters_to_context(aud->codec_ctx, stream->codecpar);
    aud->codec_ctx->thread_count = 0;
    if (avcodec_open2(aud->codec_ctx, codec, nullptr) < 0) {
        delete aud; return nullptr;
    }

    aud->channels    = aud->codec_ctx->ch_layout.nb_channels;
    aud->sample_rate = aud->codec_ctx->sample_rate;

    // For uncompressed PCM codecs the WAV header's ByteRate (nAvgBytesPerSec) is a
    // derived field (= nSamplesPerSec * nBlockAlign) that is often wrong in practice.
    // FFmpeg's ff_pcm_read_seek converts a PTS to a byte offset using:
    //   pos = pts * (codecpar->bit_rate / 8) / sample_rate
    // which should equal pts * block_align, but diverges when bit_rate was set from
    // a corrupt ByteRate.  Overwrite it with the correct formula so that av_seek_frame
    // always lands on the right byte.  We identify PCM codecs by their "pcm_" name
    // prefix (covers s16le, s24le, f32le, alaw, mulaw, …) to avoid touching
    // compressed formats (ADPCM, MP3, AAC, FLAC) where the formula doesn't hold.
    if (codec->name && strncmp(codec->name, "pcm_", 4) == 0
            && stream->codecpar->block_align > 0) {
        int64_t correct_bit_rate =
            (int64_t)aud->sample_rate * stream->codecpar->block_align * 8;
        if (correct_bit_rate != stream->codecpar->bit_rate) {
            printf("[avaudio] bit_rate fix: header=%lld  correct=%lld\n",
                   (long long)stream->codecpar->bit_rate,
                   (long long)correct_bit_rate);
            stream->codecpar->bit_rate = correct_bit_rate;
        }
    }

    // Use stream->time_base for start_time and duration rather than fmt_ctx->duration.
    // fmt_ctx->duration is derived from ByteRate and is unreliable for the same reason.
    // stream->duration = data_size / nBlockAlign and stream->time_base.den = nSamplesPerSec
    // are both independent of ByteRate and are the authoritative source.
    if (stream->start_time != AV_NOPTS_VALUE)
        aud->start_time = stream->start_time * av_q2d(stream->time_base);
    else if (aud->fmt_ctx->start_time != AV_NOPTS_VALUE)
        aud->start_time = aud->fmt_ctx->start_time / static_cast<double>(AV_TIME_BASE);

    if (stream->duration != AV_NOPTS_VALUE && stream->duration > 0)
        aud->duration = stream->duration * av_q2d(stream->time_base);
    else if (aud->fmt_ctx->duration != AV_NOPTS_VALUE && aud->fmt_ctx->duration > 0)
        aud->duration = aud->fmt_ctx->duration / static_cast<double>(AV_TIME_BASE);

    // bits_per_raw_sample: source precision (e.g. 24 for 24-bit FLAC, 0 for lossy codecs).
    // Fall back to the decoded sample format's size when not recorded.
    aud->bit_depth = aud->codec_ctx->bits_per_raw_sample;
    if (aud->bit_depth == 0)
        aud->bit_depth = av_get_bytes_per_sample(aud->codec_ctx->sample_fmt) * 8;

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
int     avloader_audio_bit_depth(AvAudioHandle h)    { return static_cast<AvAudio*>(h)->bit_depth; }
int     avloader_audio_sampling_rate(AvAudioHandle h){ return static_cast<AvAudio*>(h)->sample_rate; }

int64_t avloader_audio_get_audio(AvAudioHandle h,
                                  int64_t time_samples, int64_t duration_samples,
                                  int target_sample_rate, int target_channels,
                                  float* out_buf, int64_t buf_samples_per_channel) {
    auto* aud = static_cast<AvAudio*>(h);
    std::lock_guard<std::mutex> lock(aud->decode_mutex);

    // Convert target-rate sample positions to seconds for seeking/PTS comparison.
    const double time     = static_cast<double>(time_samples)     / target_sample_rate;
    const double duration = static_cast<double>(duration_samples) / target_sample_rate;

    AVStream* stream   = aud->fmt_ctx->streams[aud->audio_stream_idx];
    const double end_time = time + duration;

    // Build per-call SwrContext: decoded format → FLTP at target_sample_rate/target_channels.
    // Created fresh each call so there is no residual state from previous seeks.
    AVChannelLayout out_layout = {};
    av_channel_layout_default(&out_layout, target_channels);
    SwrContext* swr = nullptr;
    int swr_err = swr_alloc_set_opts2(&swr,
        &out_layout, AV_SAMPLE_FMT_FLTP, target_sample_rate,
        &aud->codec_ctx->ch_layout, aud->codec_ctx->sample_fmt, aud->sample_rate,
        0, nullptr);
    av_channel_layout_uninit(&out_layout);
    if (swr_err < 0 || !swr || swr_init(swr) < 0) {
        if (swr) swr_free(&swr);
        return -1;
    }

    // Seek slightly before target to ensure we don't miss the first frame.
    double seek_time = std::max(aud->start_time, time - 0.5);
    // Build seek_pts in stream->time_base units via av_rescale_q so that a
    // corrupted ByteRate (which may cause aud->sample_rate != stream->time_base.den)
    // does not shift the seek position.  The WAV demuxer's read_seek interprets the
    // PTS argument in stream->time_base units, not in aud->sample_rate units.
    int64_t seek_pts = av_rescale_q(
        static_cast<int64_t>((seek_time - aud->start_time) * AV_TIME_BASE),
        AV_TIME_BASE_Q,
        stream->time_base);
    if (stream->start_time != AV_NOPTS_VALUE)
        seek_pts += stream->start_time;

    if (av_seek_frame(aud->fmt_ctx, aud->audio_stream_idx, seek_pts, AVSEEK_FLAG_BACKWARD) < 0) {
        if (av_seek_frame(aud->fmt_ctx, -1,
                static_cast<int64_t>(seek_time * AV_TIME_BASE),
                AVSEEK_FLAG_BACKWARD) < 0) {
            swr_free(&swr);
            return -1;
        }
    }
    avcodec_flush_buffers(aud->codec_ctx);

    // Temporary per-frame output buffer (target rate, target channels).
    // AAC=1024, MP3=1152, Opus≤2880, FLAC/PCM≤65536 samples/frame.
    // At 4× upsampling the output can be ~4× larger, plus SRC filter delay headroom.
    const int kMaxSrcSamples = 65536;
    const int kMaxOutPerFrame = static_cast<int>(
        (static_cast<int64_t>(kMaxSrcSamples) * target_sample_rate + aud->sample_rate - 1)
        / aud->sample_rate) + 128;

    std::vector<std::vector<float>> tmp(target_channels, std::vector<float>(kMaxOutPerFrame));
    std::vector<uint8_t*> tmp_ptrs(target_channels);
    for (int c = 0; c < target_channels; c++)
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
            // Convert PTS using stream->time_base, not aud->sample_rate: for WAV PCM
            // the demuxer assigns PTS = sample_index, expressed in stream->time_base
            // units (= nSamplesPerSec).  When ByteRate is corrupted aud->sample_rate
            // may differ from stream->time_base.den, making the old /aud->sample_rate
            // form yield the wrong frame time and silently pass the wrong window check.
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

            // Convert this frame: decoded format → FLTP at target_sample_rate/target_channels.
            int converted = swr_convert(swr,
                tmp_ptrs.data(), kMaxOutPerFrame,
                const_cast<const uint8_t**>(aud->frame->data), nb);
            av_frame_unref(aud->frame);
            if (converted <= 0) continue;

            // Number of output samples to skip due to seek overshoot
            // (frame started before our target time). Computed in target-rate units.
            int skip_out = 0;
            if (frame_time < time) {
                double skip_secs = time - frame_time;
                skip_out = std::min(
                    static_cast<int>(skip_secs * target_sample_rate),
                    converted);
            }

            int use = static_cast<int>(std::min(
                static_cast<int64_t>(converted - skip_out),
                buf_samples_per_channel - collected));
            if (use <= 0) {
                if (collected >= buf_samples_per_channel) done = true;
                continue;
            }

            for (int c = 0; c < target_channels; c++) {
                std::memcpy(
                    out_buf + static_cast<int64_t>(c) * buf_samples_per_channel + collected,
                    tmp[c].data() + skip_out,
                    static_cast<size_t>(use) * sizeof(float));
            }
            collected += use;
            if (collected >= buf_samples_per_channel) done = true;
        }
    }

    // Flush SRC internal delay buffer.
    while (!done && collected < buf_samples_per_channel) {
        int flushed = swr_convert(swr, tmp_ptrs.data(), kMaxOutPerFrame, nullptr, 0);
        if (flushed <= 0) break;
        int use = static_cast<int>(std::min(
            static_cast<int64_t>(flushed),
            buf_samples_per_channel - collected));
        for (int c = 0; c < target_channels; c++) {
            std::memcpy(
                out_buf + static_cast<int64_t>(c) * buf_samples_per_channel + collected,
                tmp[c].data(),
                static_cast<size_t>(use) * sizeof(float));
        }
        collected += use;
        if (collected >= buf_samples_per_channel) done = true;
    }

    swr_free(&swr);
    return collected;
}
