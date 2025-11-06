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
        has_uniform: params.is_some(),
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

        // --- バインドグループ1 (Uniformパラメータ) の構築とセット ---
        if let Some(p) = params {
            let uniform_buffer =
                generator
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("Step {} Uniform Buffer", step_index)),
                        contents: p,
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
            let bind_group_1 = generator
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("Step {} BG Group 1", step_index)),
                    layout: &cached_pipeline.pipeline.get_bind_group_layout(1),
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buffer.as_entire_binding(),
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

// ユーザーのWGSLシェーダーの期待される形式:
/*
@group(0) @binding(0) var input_textures: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var output_texture: texture_storage_2d<rgba32float, write>;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let output_size = textureDimensions(output_texture);
    if (global_id.x >= output_size.x || global_id.y >= output_size.y) {
        return;
    }

    // 例: 最初の入力テクスチャを読み込む
    let pixel = textureLoad(input_textures[0], vec2<i32>(global_id.xy));

    // 出力テクスチャに書き込む
    textureStore(output_texture, global_id.xy, pixel);
}
*/
