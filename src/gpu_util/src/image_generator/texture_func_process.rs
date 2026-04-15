use crate::compiled_func::{CompiledTextureFunc, GpuInputTexture};
use crate::image_generator::{ImageGenerator, ProcessingState, StepOutput};
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
            StepOutput::Gpu {
                texture,
                width,
                height,
            } => {
                gpu_inputs.push(GpuInputTexture {
                    texture,
                    width,
                    height,
                });
            }
            StepOutput::Cpu { data, width, height } => {
                // CPUデータをGPUテクスチャにアップロード
                let texture = generator.get_or_create_texture(
                    width,
                    height,
                    wgpu::TextureFormat::Rgba32Float,
                    wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    Some(&format!("TextureFunc Input Upload {}", i)),
                );
                generator.queue.write_texture(
                    texture.as_image_copy(),
                    bytemuck::cast_slice(&data),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * 4 * width), // 4 (bytes/f32) * 4 (components) * width
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
                gpu_inputs.push(GpuInputTexture {
                    texture,
                    width,
                    height,
                });
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
