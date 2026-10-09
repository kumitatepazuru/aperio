use std::collections::VecDeque;
use std::sync::Arc;

use crate::compiled_func::{CompiledFunc, CpuInputImage};
use crate::image_generator::{ImageGenerator, ProcessingState, StepOutput};
use crate::image_pixel_format::ImagePixelFormat;
use crate::rhi::Texture;
use anyhow::Result;
use futures::future::join_all;
use futures::FutureExt;

/// depad済みの生バイト列を、フォーマットに応じて正規化非依存のVec<f32>へ変換する。
fn raw_bytes_to_f32(format: ImagePixelFormat, raw_pixels: &[u8]) -> Vec<f32> {
    match format {
        ImagePixelFormat::Rgba32Float => raw_pixels
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect(),
        ImagePixelFormat::Rgba16Float => raw_pixels
            .chunks_exact(2)
            .map(|c| half::f16::from_le_bytes(c.try_into().unwrap()).to_f32())
            .collect(),
        ImagePixelFormat::Rgba8Unorm => raw_pixels.iter().map(|&b| b as f32 / 255.0).collect(),
    }
}

/// GPUテクスチャをCPU側のVec<f32>(常に正規化されたRGBA、内部フォーマット非依存)へダウンロードする。
async fn download_gpu_texture(
    generator: &ImageGenerator,
    texture_to_read: &Texture,
) -> Result<(Vec<f32>, u32, u32)> {
    let width = texture_to_read.width();
    let height = texture_to_read.height();
    let format = texture_to_read.format();

    let raw_pixels = generator.device.download_texture_data(texture_to_read)?;

    Ok((raw_bytes_to_f32(format, &raw_pixels), width, height))
}

pub async fn handle_cpu_func_step(
    generator: &ImageGenerator,
    state: &mut ProcessingState,
    func: &CompiledFunc,
    params: &Option<Vec<u8>>,
    output_width: u32,
    output_height: u32,
) -> Result<ProcessingState> {
    // --- 入力データの準備 ---
    let mut download_futures = Vec::new();
    // 元の順序を保持しつつ、CPUデータとGPUダウンロード結果を区別する
    enum TempInput {
        Cpu(StepOutput),
        GpuDownload(),
    }
    let mut temp_inputs: Vec<TempInput> = Vec::with_capacity(state.len());

    for input in state.drain(..) {
        match input {
            StepOutput::Gpu { texture, .. } => {
                let future = async move { download_gpu_texture(generator, &texture).await };
                download_futures.push(future.boxed());
                temp_inputs.push(TempInput::GpuDownload());
            }
            cpu_output @ StepOutput::Cpu { .. } => {
                temp_inputs.push(TempInput::Cpu(cpu_output));
            }
        }
    }

    // GPUからのダウンロードを並列実行
    let downloaded_data_results = join_all(download_futures).await;
    let mut downloaded_data: VecDeque<_> = downloaded_data_results
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .into();

    // --- すべての入力を CpuInputImage にまとめる ---
    let mut owned_cpu_data: Vec<StepOutput> = Vec::with_capacity(temp_inputs.len());

    for temp_input in temp_inputs {
        match temp_input {
            TempInput::Cpu(cpu_output) => {
                owned_cpu_data.push(cpu_output);
            }
            TempInput::GpuDownload() => {
                let downloaded = downloaded_data.pop_front().unwrap();
                owned_cpu_data.push(StepOutput::Cpu {
                    data: Arc::new(downloaded.0),
                    width: downloaded.1,
                    height: downloaded.2,
                });
            }
        }
    }

    let cpu_inputs: Vec<CpuInputImage> = owned_cpu_data
        .iter()
        .map(|step_output| {
            if let StepOutput::Cpu { data, width, height } = step_output {
                CpuInputImage {
                    data: data.as_slice(),
                    width: *width,
                    height: *height,
                }
            } else {
                unreachable!()
            }
        })
        .collect();

    // --- CPU関数の実行 ---
    let cpu_output_data = (*func.func)(&cpu_inputs, params.as_deref())?;

    let new_state = vec![StepOutput::Cpu {
        data: Arc::new(cpu_output_data.data),
        width: output_width,
        height: output_height,
    }];

    Ok(new_state)
}
