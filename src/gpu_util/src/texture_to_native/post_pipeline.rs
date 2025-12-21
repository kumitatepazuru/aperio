use crate::image_generator::ImageGenerator;
use anyhow::Result;

/// f32_to_f16パイプラインを実行してsource_textureをdestination_textureに書き込む
pub fn execute_f32_to_f16_pipeline(
    source_texture: &wgpu::Texture,
    destination_texture: &wgpu::Texture,
    generator: &ImageGenerator,
) -> Result<()> {
    // source_textureからサイズを取得
    let width = source_texture.width();
    let height = source_texture.height();

    let source_view = source_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let destination_view = destination_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // BindGroupを作成
    let bind_group = generator
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("f32_to_f16 Bind Group"),
            layout: &generator.f32_to_f16_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&destination_view),
                },
            ],
        });

    // CommandEncoderを作成してComputePassを実行
    let mut encoder = generator
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("f32_to_f16 Command Encoder"),
        });

    {
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("f32_to_f16 Compute Pass"),
            timestamp_writes: None,
        });

        compute_pass.set_pipeline(&generator.f32_to_f16_pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);

        // ワークグループサイズは16x16 (f32_to_f16.wgslに合わせる)
        let workgroup_count_x = (width + 15) / 16;
        let workgroup_count_y = (height + 15) / 16;
        compute_pass.dispatch_workgroups(workgroup_count_x, workgroup_count_y, 1);
    }

    generator.queue.submit(std::iter::once(encoder.finish()));

    Ok(())
}
