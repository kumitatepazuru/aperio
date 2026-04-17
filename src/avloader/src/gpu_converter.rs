use std::sync::Arc;

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use gst_video::prelude::*;
use gpu_util::image_generator::ImageGenerator;

use crate::native_format::NativeGstFormat;

// ─────────────────────────────────────────────────────────────
//  YUV パラメータ（シェーダー uniform）
// ─────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct YuvParams {
    chroma_w_div: u32,
    chroma_h_div: u32,
    swap_uv:      u32,
    _pad:         u32,
}

// ─────────────────────────────────────────────────────────────
//  パイプライン + レイアウト
// ─────────────────────────────────────────────────────────────

struct Pipeline {
    cp:  wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
}

impl Pipeline {
    fn new(
        device: &wgpu::Device,
        shader_src: &str,
        bgl_entries: &[wgpu::BindGroupLayoutEntry],
        label: &str,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("{label} BGL")),
            entries: bgl_entries,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{label} PL")),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let cp = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(&format!("{label} CP")),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self { cp, bgl }
    }
}

// ─────────────────────────────────────────────────────────────
//  共通 BGL エントリ生成ヘルパー
// ─────────────────────────────────────────────────────────────

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba8Unorm,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}

// ─────────────────────────────────────────────────────────────
//  GpuFormatConverter
// ─────────────────────────────────────────────────────────────

/// 動画フォーマットに応じた GPU 変換を行うコンバーター。
///
/// セミプラナー / フルプラナー / Direct RGBA の 3 パスに対応。
pub struct GpuFormatConverter {
    semiplanar: Pipeline, // NV12/NV21/NV16/NV61
    planar: Pipeline,     // I420/YV12/I422/I444
}

impl GpuFormatConverter {
    pub fn new(device: &wgpu::Device) -> Self {
        // セミプラナー: Y(0) + UV(1) + params(2) + out(3)
        let semi = Pipeline::new(
            device,
            include_str!("shaders/yuv_semiplanar_to_rgba.wgsl"),
            &[
                tex_entry(0),
                tex_entry(1),
                uniform_entry(2),
                storage_tex_entry(3),
            ],
            "YUV SemiPlanar",
        );

        // フルプラナー: Y(0) + U(1) + V(2) + params(3) + out(4)
        let planar = Pipeline::new(
            device,
            include_str!("shaders/yuv_planar_to_rgba.wgsl"),
            &[
                tex_entry(0),
                tex_entry(1),
                tex_entry(2),
                uniform_entry(3),
                storage_tex_entry(4),
            ],
            "YUV Planar",
        );

        Self {
            semiplanar: semi,
            planar,
        }
    }

    /// GStreamer サンプルをネイティブフォーマットのまま GPU へ転送し、
    /// Rgba8Unorm テクスチャに変換して返す。
    pub fn convert(
        &self,
        gen: &ImageGenerator,
        sample: &gst::Sample,
        vinfo: &gst_video::VideoInfo,
        native_fmt: &NativeGstFormat,
        width: u32,
        height: u32,
    ) -> Result<Arc<wgpu::Texture>> {
        let buffer = sample.buffer().context("No buffer in sample")?;

        match native_fmt {
            NativeGstFormat::SemiPlanar { chroma_w_div, chroma_h_div, swap_uv, .. } => {
                self.convert_semiplanar(
                    gen, buffer, vinfo, width, height,
                    *chroma_w_div, *chroma_h_div, *swap_uv,
                )
            }
            NativeGstFormat::Planar { chroma_w_div, chroma_h_div, swap_uv, .. } => {
                self.convert_planar(
                    gen, buffer, vinfo, width, height,
                    *chroma_w_div, *chroma_h_div, *swap_uv,
                )
            }
            NativeGstFormat::DirectRgba => {
                self.convert_direct_rgba(gen, buffer, vinfo, width, height)
            }
        }
    }

    // ─ セミプラナー（NV12 系）─────────────────────────────────

    fn convert_semiplanar(
        &self,
        gen: &ImageGenerator,
        buffer: &gst::BufferRef,
        vinfo: &gst_video::VideoInfo,
        width: u32,
        height: u32,
        chroma_w_div: u32,
        chroma_h_div: u32,
        swap_uv: bool,
    ) -> Result<Arc<wgpu::Texture>> {
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, vinfo)
            .context("VideoFrameRef failed (semiplanar)")?;

        let y_data  = frame.plane_data(0).context("Y plane error")?;
        let uv_data = frame.plane_data(1).context("UV plane error")?;
        let y_stride  = frame.plane_stride()[0] as u32;
        let uv_stride = frame.plane_stride()[1] as u32;

        // Y プレーン (R8Unorm, width × height)
        let y_tex = upload_r8(gen, y_data, width, height, y_stride, "Y SemiPlanar");

        // UV プレーン (Rg8Unorm, width/w_div × height/h_div)
        let uv_w = (width  + chroma_w_div - 1) / chroma_w_div;
        let uv_h = (height + chroma_h_div - 1) / chroma_h_div;
        let uv_tex = upload_rg8(gen, uv_data, uv_w, uv_h, uv_stride, "UV SemiPlanar");

        let out_tex = make_rgba_out(gen, width, height, "SemiPlanar Out");
        let params  = YuvParams { chroma_w_div, chroma_h_div, swap_uv: swap_uv as u32, _pad: 0 };

        self.dispatch_semiplanar(gen, &y_tex, &uv_tex, &out_tex, &params, width, height);
        Ok(out_tex)
    }

    fn dispatch_semiplanar(
        &self,
        gen: &ImageGenerator,
        y_tex: &wgpu::Texture,
        uv_tex: &wgpu::Texture,
        out_tex: &wgpu::Texture,
        params: &YuvParams,
        width: u32,
        height: u32,
    ) {
        let y_view  = y_tex.create_view(&Default::default());
        let uv_view = uv_tex.create_view(&Default::default());
        let out_view = out_tex.create_view(&Default::default());

        let param_buf = gen.get_or_create_buffer(
            std::mem::size_of::<YuvParams>() as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            Some("YUV SemiPlanar Params"),
        );
        gen.queue.write_buffer(&param_buf, 0, bytemuck::bytes_of(params));

        let bg = gen.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SemiPlanar BG"),
            layout: &self.semiplanar.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&y_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&uv_view) },
                wgpu::BindGroupEntry { binding: 2, resource: param_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&out_view) },
            ],
        });

        let mut enc = gen.device.create_command_encoder(&Default::default());
        {
            let mut cp = enc.begin_compute_pass(&Default::default());
            cp.set_pipeline(&self.semiplanar.cp);
            cp.set_bind_group(0, &bg, &[]);
            cp.dispatch_workgroups((width + 15) / 16, (height + 15) / 16, 1);
        }
        gen.queue.submit(Some(enc.finish()));
    }

    // ─ フルプラナー（I420/I422/I444 系）──────────────────────

    fn convert_planar(
        &self,
        gen: &ImageGenerator,
        buffer: &gst::BufferRef,
        vinfo: &gst_video::VideoInfo,
        width: u32,
        height: u32,
        chroma_w_div: u32,
        chroma_h_div: u32,
        swap_uv: bool,
    ) -> Result<Arc<wgpu::Texture>> {
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, vinfo)
            .context("VideoFrameRef failed (planar)")?;

        let y_data = frame.plane_data(0).context("Y plane error")?;
        let p1_data = frame.plane_data(1).context("plane[1] error")?;
        let p2_data = frame.plane_data(2).context("plane[2] error")?;
        let y_stride  = frame.plane_stride()[0] as u32;
        let p1_stride = frame.plane_stride()[1] as u32;
        let p2_stride = frame.plane_stride()[2] as u32;

        let cw = (width  + chroma_w_div - 1) / chroma_w_div;
        let ch = (height + chroma_h_div - 1) / chroma_h_div;

        let y_tex  = upload_r8(gen, y_data,  width, height, y_stride,  "Y Planar");
        let p1_tex = upload_r8(gen, p1_data, cw,    ch,     p1_stride, "U/V Planar p1");
        let p2_tex = upload_r8(gen, p2_data, cw,    ch,     p2_stride, "U/V Planar p2");

        let out_tex = make_rgba_out(gen, width, height, "Planar Out");
        let params  = YuvParams { chroma_w_div, chroma_h_div, swap_uv: swap_uv as u32, _pad: 0 };

        self.dispatch_planar(gen, &y_tex, &p1_tex, &p2_tex, &out_tex, &params, width, height);
        Ok(out_tex)
    }

    fn dispatch_planar(
        &self,
        gen: &ImageGenerator,
        y_tex: &wgpu::Texture,
        p1_tex: &wgpu::Texture,
        p2_tex: &wgpu::Texture,
        out_tex: &wgpu::Texture,
        params: &YuvParams,
        width: u32,
        height: u32,
    ) {
        let y_view  = y_tex.create_view(&Default::default());
        let p1_view = p1_tex.create_view(&Default::default());
        let p2_view = p2_tex.create_view(&Default::default());
        let out_view = out_tex.create_view(&Default::default());

        let param_buf = gen.get_or_create_buffer(
            std::mem::size_of::<YuvParams>() as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            Some("YUV Planar Params"),
        );
        gen.queue.write_buffer(&param_buf, 0, bytemuck::bytes_of(params));

        let bg = gen.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Planar BG"),
            layout: &self.planar.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&y_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&p1_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&p2_view) },
                wgpu::BindGroupEntry { binding: 3, resource: param_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&out_view) },
            ],
        });

        let mut enc = gen.device.create_command_encoder(&Default::default());
        {
            let mut cp = enc.begin_compute_pass(&Default::default());
            cp.set_pipeline(&self.planar.cp);
            cp.set_bind_group(0, &bg, &[]);
            cp.dispatch_workgroups((width + 15) / 16, (height + 15) / 16, 1);
        }
        gen.queue.submit(Some(enc.finish()));
    }

    // ─ DirectRgba（videoconvert → RGBA 済みデータを直接転送）─

    fn convert_direct_rgba(
        &self,
        gen: &ImageGenerator,
        buffer: &gst::BufferRef,
        vinfo: &gst_video::VideoInfo,
        width: u32,
        height: u32,
    ) -> Result<Arc<wgpu::Texture>> {
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, vinfo)
            .context("VideoFrameRef failed (DirectRgba)")?;
        let data   = frame.plane_data(0).context("RGBA plane error")?;
        let stride = frame.plane_stride()[0] as u32;

        let tex = gen.get_or_create_texture(
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            Some("Direct RGBA"),
        );
        gen.queue.write_texture(
            tex.as_image_copy(),
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: None,
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        Ok(tex)
    }
}

// ─────────────────────────────────────────────────────────────
//  テクスチャアップロードヘルパー
// ─────────────────────────────────────────────────────────────

fn upload_r8(
    gen: &ImageGenerator,
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    label: &str,
) -> Arc<wgpu::Texture> {
    let tex = gen.get_or_create_texture(
        width,
        height,
        wgpu::TextureFormat::R8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        Some(label),
    );
    gen.queue.write_texture(
        tex.as_image_copy(),
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    tex
}

fn upload_rg8(
    gen: &ImageGenerator,
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    label: &str,
) -> Arc<wgpu::Texture> {
    let tex = gen.get_or_create_texture(
        width,
        height,
        wgpu::TextureFormat::Rg8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        Some(label),
    );
    gen.queue.write_texture(
        tex.as_image_copy(),
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    tex
}

fn make_rgba_out(gen: &ImageGenerator, width: u32, height: u32, label: &str) -> Arc<wgpu::Texture> {
    gen.get_or_create_texture(
        width,
        height,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        Some(label),
    )
}
