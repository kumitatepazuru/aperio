use anyhow::Result;

use crate::{
    compiled_slang::CompiledSlang,
    image_generator::{
        layout::{self, AperioDispatch, DispatchOutput},
        ImageGenerator, ProcessingState, StepOutput,
    },
    image_pixel_format::ImagePixelFormat,
    rhi::{self, BufferUsage, TextureUsage},
};

/// CPU側の`Vec<f32>`(RGBA32Float相当)をGPUテクスチャへアップロードする。
fn upload_cpu_data_as_texture(
    generator: &ImageGenerator,
    data: &[f32],
    width: u32,
    height: u32,
    label: &str,
) -> Result<rhi::Texture> {
    let texture = generator.get_or_create_texture(
        width,
        height,
        ImagePixelFormat::Rgba32Float,
        TextureUsage::SAMPLED | TextureUsage::STORAGE | TextureUsage::TRANSFER_DST,
        label,
    )?;

    generator
        .device
        .upload_texture_data(&texture, bytemuck::cast_slice(data))?;

    Ok(texture)
}

pub fn handle_slang_step(
    generator: &ImageGenerator,
    state: &ProcessingState,
    slang: &CompiledSlang,
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
            } => {
                let texture = upload_cpu_data_as_texture(
                    generator,
                    data,
                    *width,
                    *height,
                    &format!("Slang Step {step_index} Input {i} Upload"),
                )?;
                input_textures.push(texture);
            }
        }
    }

    // --- 出力テクスチャの準備 ---
    let output_texture = generator.get_or_create_texture(
        output_width,
        output_height,
        slang.output_format,
        TextureUsage::STORAGE
            | TextureUsage::SAMPLED
            | TextureUsage::TRANSFER_SRC
            | TextureUsage::TRANSFER_DST,
        &format!("Slang Step {step_index} Output"),
    )?;

    // --- パイプラインの取得(キャッシュ) ---
    let pipeline =
        generator.get_or_create_pipeline(slang, input_textures.len() as u32, params.is_some())?;

    // --- テクスチャビューの生成 ---
    let output_view = device.create_texture_view(&output_texture)?;
    let input_views = input_textures
        .iter()
        .map(|t| device.create_texture_view(t))
        .collect::<Result<Vec<_>>>()?;

    // --- パラメータバッファ(あれば) ---
    let params_buffer = if let Some(p) = params {
        let buf = generator.get_or_create_buffer(
            p.len().max(1) as u64,
            BufferUsage::STORAGE,
            &format!("Slang Step {step_index} Params"),
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

    layout::dispatch(
        device,
        AperioDispatch {
            pipeline: &pipeline,
            inputs: &input_views,
            output: DispatchOutput::Texture(&output_view),
            sampler: slang.sampler.as_ref(),
            params: params_buffer.as_ref(),
            workgroups: (output_width.div_ceil(16), output_height.div_ceil(16), 1),
        },
    )?;

    Ok(vec![StepOutput::Gpu {
        texture: output_texture,
        width: output_width,
        height: output_height,
    }])
}
