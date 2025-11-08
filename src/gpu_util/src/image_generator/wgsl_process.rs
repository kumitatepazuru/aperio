use crate::{
    compiled_wgsl::CompiledWgsl,
    image_generator::{ImageGenerator, PipelineCacheKey, ProcessingState, StepOutput},
};
use anyhow::Result;
use wgpu::util::DeviceExt;

pub fn handle_wgsl_step(
    generator: &ImageGenerator,
    state: &ProcessingState,
    wgsl: &CompiledWgsl,
    params: &Option<Vec<u8>>,
    step_index: usize,
    output_width: u32,
    output_height: u32,
) -> Result<(ProcessingState, Vec<wgpu::CommandEncoder>)> {
    // --- 入力データの準備 ---
    // すべての入力をGPUバッファに変換する。
    let mut encoder = generator.device.create_command_encoder(&Default::default());
    let mut input_texture_views: Vec<wgpu::TextureView> = Vec::with_capacity(state.len());

    for (i, input) in state.iter().enumerate() {
        match input {
            StepOutput::Gpu { texture, .. } => {
                input_texture_views.push(texture.create_view(&Default::default()));
            }
            StepOutput::Cpu {
                data,
                width,
                height,
            } => {
                // CPUデータをGPUにアップロード - キャッシュされたテクスチャを使用
                let texture = generator.get_or_create_texture(
                    step_index,
                    *width,
                    *height,
                    wgpu::TextureFormat::Rgba32Float,
                    wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    Some(&format!("Step {} WGSL Input Upload {}", step_index, i)),
                );
                generator.queue.write_texture(
                    texture.as_image_copy(),
                    bytemuck::cast_slice(data),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * 4 * *width), // 4 (bytes/f32) * 4 (components) * width
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: *width,
                        height: *height,
                        depth_or_array_layers: 1,
                    },
                );
                input_texture_views.push(texture.create_view(&Default::default()));
            }
        }
    }

    // --- 出力テクスチャの作成 ---
    let output_texture = generator.get_or_create_texture(
        step_index,
        output_width,
        output_height,
        wgpu::TextureFormat::Rgba32Float,
        wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        Some(&format!("Step {} Output Texture", step_index)),
    );
    let output_texture_view = output_texture.create_view(&Default::default());

    // --- パイプラインの取得 ---
    let key = PipelineCacheKey {
        id: wgsl.id.clone(),
        input_texture_count: input_texture_views.len(),
        has_storage: params.is_some(),
        has_sampler: wgsl.sampler.is_some(),
    };
    let cached_pipeline = generator.get_or_create_pipeline(&key, &wgsl.module)?;

    // --- バインドグループ0 (テクスチャ) の構築 ---
    let mut bg_entries_group0 = Vec::new();
    let input_texture_view_refs: Vec<_> = input_texture_views.iter().collect();
    if !input_texture_views.is_empty() {
        bg_entries_group0.push(wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureViewArray(&input_texture_view_refs),
        });
    }
    bg_entries_group0.push(wgpu::BindGroupEntry {
        // inputがなければbinding=0、あればbinding=1になる想定
        binding: if input_texture_views.is_empty() { 0 } else { 1 },
        resource: wgpu::BindingResource::TextureView(&output_texture_view),
    });

    // サンプラーのバインディング (存在する場合)
    if let Some(sampler) = &wgsl.sampler {
        bg_entries_group0.push(wgpu::BindGroupEntry {
            binding: if input_texture_views.is_empty() { 1 } else { 2 },
            resource: wgpu::BindingResource::Sampler(sampler.as_ref()),
        });
    }

    let bind_group_0 = generator
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("Step {} BG Group 0", step_index)),
            layout: &cached_pipeline.pipeline.get_bind_group_layout(0),
            entries: &bg_entries_group0,
        });

    // --- コンピュートパスの実行 ---
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(&format!("Step {} Compute Pass", step_index)),
            ..Default::default()
        });
        cpass.set_pipeline(&cached_pipeline.pipeline);
        cpass.set_bind_group(0, &bind_group_0, &[]);

        // --- バインドグループ1 (Storage Bufferパラメータ) の構築とセット ---
        if let Some(p) = params {
            let storage_buffer =
                generator
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("Step {} Storage Buffer", step_index)),
                        contents: p,
                        usage: wgpu::BufferUsages::STORAGE,
                    });
            let bind_group_1 = generator
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("Step {} BG Group 1", step_index)),
                    layout: &cached_pipeline.pipeline.get_bind_group_layout(1),
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &storage_buffer,
                            offset: 0,
                            size: None,
                        }),
                    }],
                });
            cpass.set_bind_group(1, &bind_group_1, &[]);
        }

        cpass.dispatch_workgroups((output_width + 15) / 16, (output_height + 15) / 16, 1);
    }

    let new_state = vec![StepOutput::Gpu {
        texture: output_texture,
        width: output_width,
        height: output_height,
    }];
    Ok((new_state, vec![encoder]))
}
