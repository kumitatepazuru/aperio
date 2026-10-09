// image_generator/final_process.rs

use std::time::Instant;

use crate::image_generator::{ImageGenerator, ProcessingState, StepOutput};
use crate::rhi::{BufferUsage, ComputeDispatch, DispatchOutput};
use anyhow::{bail, Context, Result};
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
            // テクスチャを u32 ストレージバッファへ変換して読み戻す。
            let u32_buffer_size =
                (width as u64) * (height as u64) * std::mem::size_of::<u32>() as u64;
            let u32_buffer = generator.get_or_create_buffer(
                u32_buffer_size,
                BufferUsage::STORAGE,
                "Final U32 Buffer",
            )?;
            let input_view = generator.device.create_texture_view(&texture)?;

            generator.device.dispatch_compute(ComputeDispatch {
                pipeline: &generator.post_process_pipeline,
                inputs: &[input_view],
                output: DispatchOutput::Buffer(&u32_buffer),
                sampler: None,
                params: None,
                workgroups: (width.div_ceil(16), height.div_ceil(16), 1),
            })?;

            let ptr = u32_buffer
                .mapped_ptr()
                .context("final u32 buffer should be host-mapped")?;
            Ok(unsafe {
                std::slice::from_raw_parts(ptr.as_ptr().cast::<u8>(), u32_buffer_size as usize)
                    .to_vec()
            })
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
