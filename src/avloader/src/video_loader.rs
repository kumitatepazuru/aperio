use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use gpu_util::image_generator::ImageGenerator;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

use crate::gpu_converter::GpuFormatConverter;
use crate::native_format::NativeGstFormat;
use crate::ColorFormat;

// ─────────────────────────────────────────────────────────────
//  ネイティブフォーマット検出
// ─────────────────────────────────────────────────────────────

struct ProbeResult {
    has_alpha: bool,
    native_fmt: NativeGstFormat,
}

// TODO: 動画コンテナ内に複数の動画ストリームがある場合の挙動を検討（現状は最初に見つかった動画ストリームを使う）
fn probe_native_format(path: &str) -> Result<ProbeResult> {
    let pipeline = gst::Pipeline::new();

    let src = gst::ElementFactory::make("filesrc")
        .build()
        .context("filesrc not found")?;
    src.set_property("location", path);

    let decodebin = gst::ElementFactory::make("decodebin")
        .build()
        .context("decodebin not found")?;

    let fakesink = gst::ElementFactory::make("fakesink")
        .build()
        .context("fakesink not found")?;
    fakesink.set_property("async", false);

    pipeline.add_many([&src, &decodebin, &fakesink])?;
    src.link(&decodebin)?;

    let result: Arc<std::sync::Mutex<Option<ProbeResult>>> = Arc::new(std::sync::Mutex::new(None));
    let result_cl = result.clone();
    let fakesink_cl = fakesink.clone();

    decodebin.connect_pad_added(move |_dbin, pad| {
        let caps = match pad.current_caps() {
            Some(c) => c,
            None => return,
        };
        let s = match caps.structure(0) {
            Some(s) => s,
            None => return,
        };
        if !s.name().starts_with("video/") {
            return;
        }

        let fmt_str = s.get::<&str>("format").unwrap_or("I420");
        const ALPHA_FMTS: &[&str] = &["RGBA", "BGRA", "ARGB", "ABGR", "AYUV", "VUYA"];
        let has_alpha = ALPHA_FMTS.contains(&fmt_str);
        let native_fmt = NativeGstFormat::from_gst_fmt(fmt_str);

        *result_cl.lock().unwrap() = Some(ProbeResult {
            has_alpha,
            native_fmt,
        });

        let sink_pad = fakesink_cl.static_pad("sink").unwrap();
        if !sink_pad.is_linked() {
            let _ = pad.link(&sink_pad);
        }
    });

    pipeline.set_state(gst::State::Paused)?;
    let bus = pipeline.bus().context("No bus")?;
    loop {
        let msg = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(5),
            &vec![gst::MessageType::AsyncDone, gst::MessageType::Error],
        );
        match msg {
            None => break,
            Some(m) => match m.view() {
                gst::MessageView::AsyncDone(_) => break,
                gst::MessageView::Error(e) => {
                    let _ = pipeline.set_state(gst::State::Null);
                    return Err(anyhow!("Probe error: {}", e.error()));
                }
                _ => {}
            },
        }
    }
    let _ = pipeline.set_state(gst::State::Null);

    // フォールバック（検出失敗時）
    let x = Ok(result.lock().unwrap().take().unwrap_or(ProbeResult {
        has_alpha: false,
        native_fmt: NativeGstFormat::from_gst_fmt("I420"),
    }));
    x
}

// ─────────────────────────────────────────────────────────────
//  パイプライン構築
// ─────────────────────────────────────────────────────────────

/// CPU 出力用メインパイプライン（常に RGBA 8bit）。
fn build_main_pipeline(
    path: &str,
    has_alpha: bool,
    fps_num: i32,
    fps_den: i32,
) -> Result<(gst::Pipeline, gst_app::AppSink, u32, u32)> {
    let fmt = if has_alpha { "RGBA" } else { "RGBA" }; // 常に RGBA で統一

    build_pipeline_internal(
        path, fmt, true, /* needs videoconvert */
        fps_num, fps_den, "main",
    )
}

/// GPU 出力用パイプライン（ネイティブフォーマットのまま）。
fn build_gpu_pipeline(
    path: &str,
    native_fmt: &NativeGstFormat,
    fps_num: i32,
    fps_den: i32,
) -> Result<(gst::Pipeline, gst_app::AppSink, gst_video::VideoInfo)> {
    let (pipeline, appsink, _, _) = build_pipeline_internal(
        path,
        native_fmt.gpu_pipeline_fmt(),
        native_fmt.needs_videoconvert(),
        fps_num,
        fps_den,
        "gpu",
    )?;

    let sample = appsink
        .pull_preroll()
        .map_err(|e| anyhow!("GPU pull_preroll: {e:?}"))?;
    let caps = sample.caps().context("GPU sample no caps")?;
    let vinfo = gst_video::VideoInfo::from_caps(caps).context("GPU VideoInfo")?;

    Ok((pipeline, appsink, vinfo))
}

fn build_pipeline_internal(
    path: &str,
    output_fmt: &str,
    needs_videoconvert: bool,
    fps_num: i32,
    fps_den: i32,
    tag: &str,
) -> Result<(gst::Pipeline, gst_app::AppSink, u32, u32)> {
    let pipeline = gst::Pipeline::new();

    let src = gst::ElementFactory::make("filesrc").build()?;
    src.set_property("location", path);

    let decodebin = gst::ElementFactory::make("decodebin").build()?;
    let queue = gst::ElementFactory::make("queue").build()?;

    let appsink = gst_app::AppSink::builder()
        .max_buffers(1)
        .drop(false)
        .sync(false)
        .build();

    let caps_str = format!("video/x-raw,format={output_fmt},framerate={fps_num}/{fps_den}");
    let capsfilter = gst::ElementFactory::make("capsfilter").build()?;
    capsfilter.set_property(
        "caps",
        &gst::Caps::from_str(&caps_str).context("Invalid caps")?,
    );

    let videorate = gst::ElementFactory::make("videorate").build()?;

    if needs_videoconvert {
        let videoconvert = gst::ElementFactory::make("videoconvert").build()?;
        pipeline.add_many([
            &src,
            &decodebin,
            &queue,
            &videoconvert,
            &videorate,
            &capsfilter,
            appsink.upcast_ref(),
        ])?;
        src.link(&decodebin)?;
        gst::Element::link_many([&queue, &videoconvert, &videorate, &capsfilter])?;
    } else {
        pipeline.add_many([
            &src,
            &decodebin,
            &queue,
            &videorate,
            &capsfilter,
            appsink.upcast_ref(),
        ])?;
        src.link(&decodebin)?;
        gst::Element::link_many([&queue, &videorate, &capsfilter])?;
    }
    capsfilter.link(&appsink)?;

    let queue_cl = queue.clone();
    decodebin.connect_pad_added(move |_dbin, pad| {
        let caps = match pad.current_caps() {
            Some(c) => c,
            None => return,
        };
        let s = match caps.structure(0) {
            Some(s) => s,
            None => return,
        };
        if !s.name().starts_with("video/") {
            return;
        }
        let sink = queue_cl.static_pad("sink").unwrap();
        if !sink.is_linked() {
            let _ = pad.link(&sink);
        }
    });

    pipeline
        .set_state(gst::State::Paused)
        .context(format!("{tag} pipeline PAUSED failed"))?;
    wait_async_done(&pipeline, 15)?;

    // 解像度を preroll サンプルから取得
    let sample = appsink
        .pull_preroll()
        .map_err(|e| anyhow!("{tag} pull_preroll: {e:?}"))?;
    let caps = sample.caps().context("preroll no caps")?;
    let vinfo = gst_video::VideoInfo::from_caps(caps).context("VideoInfo failed")?;
    let width = vinfo.width();
    let height = vinfo.height();

    Ok((pipeline, appsink, width, height))
}

// ─────────────────────────────────────────────────────────────
//  シーク & preroll 取得
// ─────────────────────────────────────────────────────────────

fn seek_and_pull_preroll(
    pipeline: &gst::Pipeline,
    appsink: &gst_app::AppSink,
    frame_number: u64,
    fps_num: i32,
    fps_den: i32,
) -> Result<gst::Sample> {
    let timestamp_ns = if frame_number <= 1 {
        0u64
    } else {
        let fps = fps_num as f64 / fps_den as f64;
        ((frame_number - 1) as f64 / fps * 1_000_000_000.0) as u64
    };

    pipeline
        .seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
            gst::ClockTime::from_nseconds(timestamp_ns),
        )
        .map_err(|e| anyhow!("seek_simple: {e}"))?;

    wait_async_done(pipeline, 10)?;

    appsink
        .pull_preroll()
        .map_err(|e| anyhow!("pull_preroll after seek: {e:?}"))
}

fn wait_async_done(pipeline: &gst::Pipeline, secs: u64) -> Result<()> {
    let bus = pipeline.bus().context("No bus")?;
    loop {
        let msg = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(secs),
            &vec![
                gst::MessageType::AsyncDone,
                gst::MessageType::Error,
                gst::MessageType::Eos,
            ],
        );
        match msg {
            None => return Err(anyhow!("Timeout waiting ASYNC_DONE")),
            Some(m) => match m.view() {
                gst::MessageView::AsyncDone(_) => return Ok(()),
                gst::MessageView::Error(e) => return Err(anyhow!("Pipeline error: {}", e.error())),
                gst::MessageView::Eos(_) => return Err(anyhow!("Unexpected EOS")),
                _ => {}
            },
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  FPS ユーティリティ
// ─────────────────────────────────────────────────────────────

fn fps_to_fraction(fps: f64) -> (i32, i32) {
    const COMMON: &[(f64, i32, i32)] = &[
        (23.976, 24000, 1001),
        (24.0, 24, 1),
        (25.0, 25, 1),
        (29.97, 30000, 1001),
        (30.0, 30, 1),
        (48.0, 48, 1),
        (50.0, 50, 1),
        (59.94, 60000, 1001),
        (60.0, 60, 1),
        (120.0, 120, 1),
    ];
    for &(f, n, d) in COMMON {
        if (fps - f).abs() < 0.01 {
            return (n, d);
        }
    }
    ((fps * 1000.0).round() as i32, 1000)
}

// ─────────────────────────────────────────────────────────────
//  VideoLoader
// ─────────────────────────────────────────────────────────────
// TODO: 16bit出力ができないわけではないので将来対応

/// 1 つの動画ファイルを扱うローダー。
///
/// * `get_frame`         → フレームを RGBA u8 配列で返す（クローンなし）
/// * `get_texture_frame` → フレームをネイティブ YUV のまま GPU に転送して Rgba8Unorm テクスチャで返す
pub struct VideoLoader {
    // CPU 出力パイプライン（常に RGBA 8bit）
    main_pipeline: gst::Pipeline,
    main_appsink: gst_app::AppSink,

    // GPU 出力パイプライン（ネイティブフォーマット）
    gpu_pipeline: gst::Pipeline,
    gpu_appsink: gst_app::AppSink,
    gpu_vinfo: gst_video::VideoInfo,
    native_fmt: NativeGstFormat,

    width: u32,
    height: u32,
    fps_num: i32,
    fps_den: i32,
    color_format: ColorFormat,

    image_generator: ImageGenerator,
    gpu_converter: GpuFormatConverter,
}

impl VideoLoader {
    /// 新しい VideoLoader を作成する。
    ///
    /// # Arguments
    /// * `path`            : 動画ファイルパス
    /// * `target_fps`      : 出力 FPS（videorate で変換）
    /// * `image_generator` : GPU リソース管理
    pub fn new(
        path: impl AsRef<std::path::Path>,
        target_fps: f64,
        image_generator: ImageGenerator,
    ) -> Result<Self> {
        gst::init().context("GStreamer init failed")?;

        let path_str = path.as_ref().to_str().context("Path not UTF-8")?;

        // ─ ネイティブフォーマット検出 ─
        let probe = probe_native_format(path_str)?;
        let color_format = if probe.has_alpha {
            ColorFormat::RgbaUnorm
        } else {
            ColorFormat::RgbUnorm
        };
        let native_fmt = probe.native_fmt;

        let (fps_num, fps_den) = fps_to_fraction(target_fps);

        // ─ CPU パイプライン（常に RGBA 8bit 出力）─
        let (main_pipeline, main_appsink, width, height) =
            build_main_pipeline(path_str, probe.has_alpha, fps_num, fps_den)?;

        // ─ GPU パイプライン（ネイティブフォーマット出力）─
        let (gpu_pipeline, gpu_appsink, gpu_vinfo) =
            build_gpu_pipeline(path_str, &native_fmt, fps_num, fps_den)?;

        // GPU コンバーター（セミプラナー / フルプラナー / DirectRgba シェーダー）
        let gpu_converter = GpuFormatConverter::new(&image_generator.device);

        Ok(Self {
            main_pipeline,
            main_appsink,
            gpu_pipeline,
            gpu_appsink,
            gpu_vinfo,
            native_fmt,
            width,
            height,
            fps_num,
            fps_den,
            color_format,
            image_generator,
            gpu_converter,
        })
    }

    // ─ プロパティ ─────────────────────────────────────────────

    pub fn get_width(&self) -> u32 {
        self.width
    }
    pub fn get_height(&self) -> u32 {
        self.height
    }
    pub fn get_color_format(&self) -> ColorFormat {
        self.color_format
    }

    // ─ フレーム取得 ───────────────────────────────────────────

    /// 指定フレーム（1 始まり）を RGBA u8 配列で返す。
    ///
    /// GStreamer バッファ → Vec への 1 回コピーのみ（内部クローンなし）。
    pub fn get_frame(&self, frame_number: u64) -> Result<Vec<u8>> {
        let sample = seek_and_pull_preroll(
            &self.main_pipeline,
            &self.main_appsink,
            frame_number,
            self.fps_num,
            self.fps_den,
        )?;

        let buffer = sample.buffer().context("No buffer in sample")?;
        let mapped = buffer.map_readable().context("Buffer map failed")?;
        // GStreamer バッファ → Vec（1 回コピー）
        Ok(mapped.as_slice().to_vec())
    }

    /// 指定フレームを GPU テクスチャ（Rgba8Unorm）で返す。
    ///
    /// ネイティブ YUV フォーマットのまま GPU に転送し、chroma subsampling を変えずに変換。
    pub fn get_texture_frame(&self, frame_number: u64) -> Result<Arc<wgpu::Texture>> {
        let sample = seek_and_pull_preroll(
            &self.gpu_pipeline,
            &self.gpu_appsink,
            frame_number,
            self.fps_num,
            self.fps_den,
        )?;

        self.gpu_converter.convert(
            &self.image_generator,
            &sample,
            &self.gpu_vinfo,
            &self.native_fmt,
            self.width,
            self.height,
        )
    }
}

impl Drop for VideoLoader {
    fn drop(&mut self) {
        let _ = self.main_pipeline.set_state(gst::State::Null);
        let _ = self.gpu_pipeline.set_state(gst::State::Null);
    }
}
