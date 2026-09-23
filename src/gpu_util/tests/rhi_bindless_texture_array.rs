// 可変長テクスチャ配列と非uniform添字参照パターンを、実際のVulkanディスクリプタで再現して検証する。
//
// 4枚の1x1テクスチャ(それぞれ異なる値)を1つの可変長Texture2D配列
// バインディングにまとめ、各スレッドがStructuredBuffer<Params>から読んだ
// (スレッドごとに異なる = non-uniformな)添字でtex[NonUniformResourceIndex(idx)]
// を参照し、出力バッファへ書き戻す。期待通りの値が読めれば、bindless/
// 可変長配列/non-uniform添字参照が一貫して機能していることになる。

use ash::vk;
use gpu_allocator::MemoryLocation;
use gpu_util::compiled_slang::compile_slang_to_spirv;
use gpu_util::rhi::vulkan::descriptor::DescriptorBindingDesc;
use gpu_util::rhi::vulkan::VulkanDevice;
use std::sync::Arc;

const TEXTURE_COUNT: usize = 4;

/// 1x1のRGBA32Floatテクスチャを1枚作り、`value`で塗りつぶして
/// シェーダーから読み取り可能な状態(SHADER_READ_ONLY_OPTIMAL)にする。
fn create_filled_texture(
    device: &Arc<VulkanDevice>,
    value: [f32; 4],
    name: &str,
) -> (gpu_util::rhi::vulkan::Texture, gpu_util::rhi::vulkan::TextureView) {
    let format = vk::Format::R32G32B32A32_SFLOAT;
    let texture = device
        .create_texture(
            1,
            1,
            format,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            name,
        )
        .expect("texture creation should succeed");

    let staging = device
        .create_buffer(
            16,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            "staging buffer",
        )
        .expect("staging buffer creation should succeed");
    unsafe {
        let ptr = staging
            .mapped_ptr()
            .expect("staging buffer should be host-mapped")
            .cast::<f32>();
        for (i, v) in value.iter().enumerate() {
            ptr.as_ptr().add(i).write(*v);
        }
    }

    let pool = device.create_command_pool().expect("command pool");
    let cb = device
        .allocate_command_buffer(pool)
        .expect("command buffer");

    device
        .submit_and_wait(cb, |cb| unsafe {
            let subresource = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            };

            let to_transfer_dst = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(texture.handle())
                .subresource_range(subresource)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            device.device.cmd_pipeline_barrier(
                cb,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_transfer_dst],
            );

            let region = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width: 1,
                    height: 1,
                    depth: 1,
                },
            };
            device.device.cmd_copy_buffer_to_image(
                cb,
                staging.handle(),
                texture.handle(),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );

            let to_shader_read = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(texture.handle())
                .subresource_range(subresource)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);
            device.device.cmd_pipeline_barrier(
                cb,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_shader_read],
            );
        })
        .expect("upload commands should succeed");

    unsafe { device.device.destroy_command_pool(pool, None) };

    let view = device
        .create_texture_view(&texture, vk::ImageAspectFlags::COLOR)
        .expect("texture view creation should succeed");

    (texture, view)
}

#[test]
fn reads_variable_length_texture_array_with_nonuniform_indices() {
    let device = match VulkanDevice::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "Skipping reads_variable_length_texture_array_with_nonuniform_indices: \
                 no Vulkan device ({e:#})"
            );
            return;
        }
    };

    // 各テクスチャに識別しやすい一意な値を仕込む。
    let textures: Vec<_> = (0..TEXTURE_COUNT)
        .map(|i| {
            let v = (i as f32 + 1.0) * 10.0;
            create_filled_texture(&device, [v, v + 1.0, v + 2.0, v + 3.0], "bindless test texture")
        })
        .collect();

    let source = r#"
        [[vk::binding(2, 0)]]
        Texture2D<float4> tex[];

        struct Params { uint index; };
        [[vk::binding(1, 0)]]
        StructuredBuffer<Params> params;

        [[vk::binding(0, 0)]]
        RWStructuredBuffer<float4> output;

        [shader("compute")]
        [numthreads(4, 1, 1)]
        void main(uint3 tid : SV_DispatchThreadID) {
            uint idx = params[tid.x].index;
            output[tid.x] = tex[NonUniformResourceIndex(idx)].Load(int3(0, 0, 0));
        }
    "#;

    let spirv = compile_slang_to_spirv(
        "rhi_bindless_texture_array_test",
        "rhi_bindless_texture_array_test.slang",
        source,
        "main",
        &[],
        &[],
    )
    .expect("bindless shader should compile to SPIR-V");

    // 各スレッドに割り当てる添字を、あえて連番でない(=uniformでない)順序にする。
    let indices: [u32; TEXTURE_COUNT] = [2, 0, 3, 1];

    let params_buffer = device
        .create_buffer(
            (TEXTURE_COUNT * std::mem::size_of::<u32>()) as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
            "params buffer",
        )
        .expect("params buffer creation should succeed");
    unsafe {
        let ptr = params_buffer
            .mapped_ptr()
            .expect("params buffer should be host-mapped")
            .cast::<u32>();
        for (i, idx) in indices.iter().enumerate() {
            ptr.as_ptr().add(i).write(*idx);
        }
    }

    let output_size = (TEXTURE_COUNT * 4 * std::mem::size_of::<f32>()) as u64;
    let output_buffer = device
        .create_buffer(
            output_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
            "output buffer",
        )
        .expect("output buffer creation should succeed");

    let bindings = [
        DescriptorBindingDesc {
            binding: 0,
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::COMPUTE,
            variable_count: false,
        },
        DescriptorBindingDesc {
            binding: 1,
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::COMPUTE,
            variable_count: false,
        },
        DescriptorBindingDesc {
            binding: 2,
            descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
            descriptor_count: TEXTURE_COUNT as u32,
            stage_flags: vk::ShaderStageFlags::COMPUTE,
            variable_count: true,
        },
    ];

    let set_layout = device
        .create_descriptor_set_layout(&bindings)
        .expect("descriptor set layout creation should succeed");
    let pool = device
        .create_descriptor_pool(&bindings, 1)
        .expect("descriptor pool creation should succeed");
    let set = device
        .allocate_descriptor_set(pool, set_layout, Some(TEXTURE_COUNT as u32))
        .expect("descriptor set allocation should succeed");

    device.write_buffer_descriptor(
        set,
        0,
        vk::DescriptorType::STORAGE_BUFFER,
        output_buffer.handle(),
        0,
        output_size,
    );
    device.write_buffer_descriptor(
        set,
        1,
        vk::DescriptorType::STORAGE_BUFFER,
        params_buffer.handle(),
        0,
        (TEXTURE_COUNT * std::mem::size_of::<u32>()) as u64,
    );
    for (i, (_, view)) in textures.iter().enumerate() {
        device.write_image_descriptor(
            set,
            2,
            i as u32,
            vk::DescriptorType::SAMPLED_IMAGE,
            view.handle(),
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::Sampler::null(),
        );
    }

    let (pipeline, layout) = device
        .create_compute_pipeline_from_spirv(&spirv, "main", &[set_layout])
        .expect("compute pipeline creation should succeed");

    let command_pool = device.create_command_pool().expect("command pool creation should succeed");
    let command_buffer = device
        .allocate_command_buffer(command_pool)
        .expect("command buffer allocation should succeed");

    device
        .submit_and_wait(command_buffer, |cb| unsafe {
            device
                .device
                .cmd_bind_pipeline(cb, vk::PipelineBindPoint::COMPUTE, pipeline);
            device.device.cmd_bind_descriptor_sets(
                cb,
                vk::PipelineBindPoint::COMPUTE,
                layout,
                0,
                &[set],
                &[],
            );
            device.device.cmd_dispatch(cb, 1, 1, 1);
        })
        .expect("dispatch should succeed");

    let ptr = output_buffer
        .mapped_ptr()
        .expect("output buffer should be host-mapped")
        .cast::<f32>();
    for (thread, &texture_index) in indices.iter().enumerate() {
        let expected_base = (texture_index as f32 + 1.0) * 10.0;
        for c in 0..4usize {
            let got = unsafe { ptr.as_ptr().add(thread * 4 + c).read() };
            let expected = expected_base + c as f32;
            assert!(
                (got - expected).abs() < 1e-4,
                "thread {thread} (reading texture {texture_index}) channel {c}: \
                 expected {expected}, got {got}"
            );
        }
    }

    unsafe {
        device.device.destroy_command_pool(command_pool, None);
        device.destroy_pipeline(pipeline, layout);
        device.device.destroy_descriptor_pool(pool, None);
        device.device.destroy_descriptor_set_layout(set_layout, None);
    }
}
