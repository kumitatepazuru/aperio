// image_generator/final_process.rs

use std::time::Instant;

use crate::image_generator::{ImageGenerator, ProcessingState, StepOutput};
use anyhow::{Context, Result, bail};
use futures::channel::oneshot;
use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::{ParallelSlice, ParallelSliceMut},
};

#[inline(always)]
fn f32_to_u8_clamped(x: f32) -> u8 {
    // 0..255 にクリップしてから u8 へ（切り捨て）
    let y = (x * 255.0).max(0.0).min(255.0);
    y as u8
}

/// パイプライン全体の最終処理を担当します。
pub async fn handle_final_process(
    generator: &ImageGenerator,
    final_state: ProcessingState,
) -> Result<Vec<u8>> {
    // このコードは、元の image_generator.rs の generate メソッドの
    // ループ後の最終処理部分から移動したものです。
    let final_state = if final_state.len() != 1 {
        bail!("Final processing state must contain exactly one item.");
    } else {
        final_state
            .into_iter()
            .next()
            .context("Failed to get final state item")?
    };

    match final_state {
        StepOutput::Gpu {
            texture,
            width,
            height,
        } => {
            // --- 最終的なGPUテクスチャをu8配列に変換する ---

            // 1. シェーダーが書き込むためのu32ストレージバッファを作成（キャッシュ使用）
            let u32_buffer_size = (width * height * std::mem::size_of::<u32>() as u32) as u64;
            let final_u32_buffer = generator.get_or_create_buffer(
                u32_buffer_size,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                Some("Final U32 Buffer"),
            );

            // 2. バインドグループを作成
            // ImageGenerator::newで作成したレイアウトに適合させる
            let input_texture_view = texture.create_view(&Default::default());
            let bind_group = generator
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Post Process Bind Group"),
                    layout: &generator.post_process_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&input_texture_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: final_u32_buffer.as_entire_binding(),
                        },
                    ],
                });

            let mut encoder =
                generator
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Final Process Encoder"),
                    });

            // 3. コンピュートパスを実行して、テクスチャ->u32バッファ変換を行う
            {
                let mut cpass = encoder.begin_compute_pass(&Default::default());
                cpass.set_pipeline(&generator.post_process_pipeline);
                cpass.set_bind_group(0, &bind_group, &[]);
                // ディスパッチサイズは最終的な画像の解像度に基づく
                cpass.dispatch_workgroups((width + 15) / 16, (height + 15) / 16, 1);
            }

            // 4. 結果をCPUに読み戻す（キャッシュ使用）
            let readback_buffer = generator.get_or_create_buffer(
                u32_buffer_size,
                wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                Some("Final Readback Buffer"),
            );

            encoder.copy_buffer_to_buffer(
                &final_u32_buffer,
                0,
                &readback_buffer,
                0,
                u32_buffer_size,
            );

            // 5. コマンドをサブミットし、マッピングを待つ
            generator.queue.submit(Some(encoder.finish()));

            let buffer_slice = readback_buffer.slice(..);
            let (tx, rx) = oneshot::channel();
            buffer_slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());

            generator.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })?;

            rx.await
                .context("Failed to receive buffer mapping result")??;

            let data = buffer_slice.get_mapped_range();
            let result = data.to_vec();
            drop(data); // get_mapped_rangeの借用を解除
            readback_buffer.unmap();

            Ok(result)
        }
        StepOutput::Cpu {
            data,
            width,
            height,
        } => {
            // パフォーマンス計測
            let start_time = Instant::now();

            let pixel_count = (width as usize) * (height as usize);
            let n = pixel_count * 4;
            let mut result_bytes = vec![0u8; n];

            result_bytes
                .par_chunks_exact_mut(4)
                .zip_eq(data.par_chunks_exact(4))
                .for_each(|(dst, src)| {
                    dst[0] = f32_to_u8_clamped(src[0]);
                    dst[1] = f32_to_u8_clamped(src[1]);
                    dst[2] = f32_to_u8_clamped(src[2]);
                    dst[3] = f32_to_u8_clamped(src[3]);
                });

            println!(
                "Final CPU post-processing completed in {:.2?}.",
                start_time.elapsed()
            );
            Ok(result_bytes)
        }
    }
}
