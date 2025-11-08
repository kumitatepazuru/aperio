use std::collections::VecDeque;
use std::sync::Arc;

use crate::compiled_func::{CompiledFunc, CpuInputImage};
use crate::image_generator::{ImageGenerator, ProcessingState, StepOutput};
use anyhow::Result;
use futures::channel::oneshot;
use futures::future::join_all;
use futures::FutureExt;

async fn download_gpu_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_to_read: &Arc<wgpu::Texture>,
) -> Result<(Vec<f32>, u32, u32)> {
    let (width, height) = (texture_to_read.width(), texture_to_read.height());
    let row_size = width * std::mem::size_of::<[f32; 4]>() as u32;
    let bytes_per_row = ((row_size + 255) / 256) * 256;
    let readback_buffer_size = (bytes_per_row * height) as u64;

    let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Readback Buffer"),
        size: readback_buffer_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&Default::default());
    // bytes_per_rowを256の倍数に揃える
    encoder.copy_texture_to_buffer(
        texture_to_read.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(Some(encoder.finish()));

    let buffer_slice = readback_buffer.slice(..);
    let (tx, rx) = oneshot::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    rx.await??;

    let data = buffer_slice.get_mapped_range();

    // パディングを外して詰め直し
    let mut pixels = vec![0u8; (row_size * height) as usize];
    for y in 0..height as usize {
        let src = &data[y * bytes_per_row as usize..y * bytes_per_row as usize + row_size as usize];
        let dst = &mut pixels[y * row_size as usize..(y + 1) * row_size as usize];
        dst.copy_from_slice(src);
    }
    Ok((bytemuck::cast_slice(&data).to_vec(), width, height))
}

pub async fn handle_cpu_func_step(
    generator: &ImageGenerator,
    state: &mut ProcessingState,
    func: &CompiledFunc,
    params: &Option<Vec<u8>>,
    output_width: u32,
    output_height: u32,
    all_encoders: &mut Vec<wgpu::CommandEncoder>,
) -> Result<(ProcessingState, Vec<wgpu::CommandEncoder>)> {
    // CPU関数処理では事前にすべてのエンコーダをsubmitする
    if !all_encoders.is_empty() {
        generator
            .queue
            .submit(all_encoders.drain(..).map(|e| e.finish()));
    }

    // --- 入力データの準備 ---
    let mut download_futures = Vec::new();
    // 元の順序を保持しつつ、CPUデータとGPUダウンロード結果を区別する
    enum TempInput {
        Cpu(StepOutput),
        GpuDownload(), // downloaded_dataのインデックス
    }
    let mut temp_inputs: Vec<TempInput> = Vec::with_capacity(state.len());

    for input in state.drain(..) {
        match input {
            StepOutput::Gpu { texture, .. } => {
                // generatorの中身をcloneして使う
                let device = &generator.device;
                let queue = &generator.queue;

                // ダウンロード処理をFutureとして登録
                let texture_clone = texture.clone();
                let future =
                    async move { download_gpu_texture(device, queue, &texture_clone).await };
                download_futures.push(future.boxed());
                // プレースホルダーを登録
                temp_inputs.push(TempInput::GpuDownload());
            }
            cpu_output @ StepOutput::Cpu { .. } => {
                // CPUデータはそのままプレースホルダーとして登録
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
    let mut owned_cpu_data: Vec<StepOutput> = Vec::with_capacity(temp_inputs.len()); // 所有権を保持

    for temp_input in temp_inputs {
        match temp_input {
            TempInput::Cpu(cpu_output) => {
                owned_cpu_data.push(cpu_output);
            }
            TempInput::GpuDownload() => {
                // ダウンロード結果を先頭から取り出し、所有権を移す
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
            if let StepOutput::Cpu {
                data,
                width,
                height,
            } = step_output
            {
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

    Ok((new_state, Vec::new()))
}
