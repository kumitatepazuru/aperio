use crate::{image_generator::ImageGenerator, SharedTextureFormat};
use anyhow::Result;

/// Render passを使ってsource_textureをdestination_textureに書き込む
/// Windows/Linux共通の処理
pub fn blit_texture_with_render_pass(
    source_texture: &wgpu::Texture,
    format: &SharedTextureFormat,
    destination_texture: &wgpu::Texture,
    generator: &ImageGenerator,
) -> Result<()> {
    let pipeline = match format {
        SharedTextureFormat::Rgba16Float => &generator.blit_f32_to_f16_pipeline,
        SharedTextureFormat::Bgra8Unorm => &generator.blit_f32_to_bgra8_pipeline,
    };

    // source textureのビューを作成
    let src_view = source_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let dst_view = destination_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let bind_group = generator
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blit Source->Destination BindGroup"),
            layout: &pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&generator.blit_sampler),
                },
            ],
        });

    let mut encoder = generator
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Blit Source->Destination Command Encoder"),
        });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Blit Source->Destination RenderPass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &dst_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }

    generator.queue.submit(std::iter::once(encoder.finish()));

    Ok(())
}
