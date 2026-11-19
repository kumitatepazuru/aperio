use anyhow::{bail, Result};

use crate::{
    compiled_shader::CompiledShader,
    image_generator::{
        shader_process::upload_cpu_data_as_texture, ImageGenerator, ProcessingState, StepOutput,
    },
    rhi::{self, BufferUsage, Draw, RenderTargetInit, Resource, ResourceBinding, TextureUsage},
};

/// ラスターシェーダー(`ShaderKind::Raster`)のステップを実行する。
///
/// 出力テクスチャは1枚目の入力の内容で初期化してから(入力が無ければ透明でクリア)、
/// `vertex_count`頂点ぶんの三角形リストを描画する。入力テクスチャはすべてset 0に固定長で束縛する。
pub fn handle_raster_step(
    generator: &ImageGenerator,
    state: &ProcessingState,
    shader: &CompiledShader,
    vertex_count: u32,
    params: &Option<Vec<u8>>,
    step_index: usize,
    output_width: u32,
    output_height: u32,
) -> Result<ProcessingState> {
    let device = &generator.device;

    // --- 入力テクスチャの準備(CPUデータはGPUへアップロード) ---
    let mut input_textures: Vec<rhi::Texture> = Vec::with_capacity(state.len());
    for (i, input) in state.iter().enumerate() {
        match input {
            StepOutput::Gpu { texture, .. } => input_textures.push(texture.clone()),
            StepOutput::Cpu {
                data,
                width,
                height,
            } => input_textures.push(upload_cpu_data_as_texture(
                generator,
                data,
                *width,
                *height,
                &format!("Raster Step {step_index} Input {i} Upload"),
            )?),
        }
    }

    // --- 出力テクスチャ(カラーターゲット)の準備 ---
    let output_texture = generator.get_or_create_texture(
        output_width,
        output_height,
        shader.output_format,
        TextureUsage::COLOR_TARGET
            | TextureUsage::SAMPLED
            | TextureUsage::TRANSFER_SRC
            | TextureUsage::TRANSFER_DST,
        &format!("Raster Step {step_index} Output"),
    )?;

    // --- 下地(1枚目の入力)の決定 ---
    let init = match input_textures.first() {
        None => RenderTargetInit::Clear([0.0; 4]),
        Some(base) => {
            if (base.width(), base.height()) != (output_width, output_height) {
                bail!(
                    "Raster shader '{}': the first input ({}x{}) is used as the render target's \
                     initial content, so it must match the output size ({}x{})",
                    shader.name,
                    base.width(),
                    base.height(),
                    output_width,
                    output_height
                );
            }
            if base.format() == output_texture.format() {
                RenderTargetInit::CopyFrom(base)
            } else {
                // フォーマットが違う場合はコピーできないので、変換付きの等倍描画で写してから重ねる
                generator.blit_to_texture(base, &output_texture)?;
                RenderTargetInit::Load
            }
        }
    };

    let pipeline = generator.get_or_create_raster_pipeline(
        shader,
        input_textures.len() as u32,
        params.is_some(),
    )?;

    let output_view = device.create_texture_view(&output_texture)?;
    let input_views = input_textures
        .iter()
        .map(|t| device.create_texture_view(t))
        .collect::<Result<Vec<_>>>()?;

    let params_buffer = if let Some(p) = params {
        let buf = generator.get_or_create_buffer(
            p.len().max(1) as u64,
            BufferUsage::STORAGE,
            &format!("Raster Step {step_index} Params"),
        )?;
        unsafe {
            let ptr = buf
                .mapped_ptr()
                .ok_or_else(|| anyhow::anyhow!("params buffer should be host-mapped"))?;
            std::ptr::copy_nonoverlapping(p.as_ptr(), ptr.as_ptr().cast(), p.len());
        }
        Some(buf)
    } else {
        None
    };

    let input_bindings: Vec<ResourceBinding<'_>> = input_views
        .iter()
        .enumerate()
        .map(|(i, view)| ResourceBinding {
            binding: i as u32,
            resource: Resource::Texture(view),
        })
        .collect();
    let mut res_bindings: Vec<ResourceBinding<'_>> = Vec::new();
    if let Some(sampler) = shader.sampler.as_ref() {
        res_bindings.push(ResourceBinding {
            binding: res_bindings.len() as u32,
            resource: Resource::Sampler(sampler),
        });
    }
    if let Some(buffer) = params_buffer.as_ref() {
        res_bindings.push(ResourceBinding {
            binding: res_bindings.len() as u32,
            resource: Resource::Buffer(buffer),
        });
    }

    // RasterShape::layoutと同じく、set 1は中身がある場合だけ持つ
    let sets: Vec<&[ResourceBinding<'_>]> = if res_bindings.is_empty() {
        vec![&input_bindings]
    } else {
        vec![&input_bindings, &res_bindings]
    };

    device.render_pass_with_init(
        &output_view,
        init,
        &[Draw {
            pipeline: &pipeline,
            sets: &sets,
            vertex_buffers: &[],
            vertex_count,
            instance_count: 1,
        }],
    )?;

    Ok(vec![StepOutput::Gpu {
        texture: output_texture,
        width: output_width,
        height: output_height,
    }])
}
