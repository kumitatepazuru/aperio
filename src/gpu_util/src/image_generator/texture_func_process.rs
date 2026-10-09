use crate::compiled_func::{CompiledTextureFunc, GpuInputTexture};
use crate::image_generator::{ImageGenerator, ProcessingState, StepOutput};
use crate::image_pixel_format::ImagePixelFormat;
use crate::rhi::TextureUsage;
use anyhow::Result;

pub async fn handle_texture_func_step(
    generator: &ImageGenerator,
    state: &mut ProcessingState,
    func: &CompiledTextureFunc,
    params: &Option<Vec<u8>>,
    output_width: u32,
    output_height: u32,
) -> Result<ProcessingState> {
    // stateをすべてGPUテクスチャに変換する
    let mut gpu_inputs: Vec<GpuInputTexture> = Vec::with_capacity(state.len());

    for (i, input) in state.drain(..).enumerate() {
        match input {
            StepOutput::Gpu { texture, width, height } => {
                gpu_inputs.push(GpuInputTexture { texture, width, height });
            }
            StepOutput::Cpu { data, width, height } => {
                // CPUデータをGPUテクスチャにアップロード
                let texture = generator.get_or_create_texture(
                    width,
                    height,
                    ImagePixelFormat::Rgba32Float,
                    TextureUsage::SAMPLED | TextureUsage::STORAGE | TextureUsage::TRANSFER_DST,
                    &format!("TextureFunc Input Upload {i}"),
                )?;

                generator
                    .device
                    .upload_texture_data(&texture, bytemuck::cast_slice(&data))?;

                gpu_inputs.push(GpuInputTexture { texture, width, height });
            }
        }
    }

    let output = (*func.func)(gpu_inputs, params.as_deref())?;

    Ok(vec![StepOutput::Gpu {
        texture: output.texture,
        width: output_width,
        height: output_height,
    }])
}
