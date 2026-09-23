// Slangでコンパイルしたコンピュートシェーダーを実際にVulkanパイプラインへ
// 変換し、ディスクリプタセット経由でストレージバッファを束縛して
// dispatchし、結果をCPUから読み戻せることを確認する
// (compiled_slang -> descriptor -> pipeline -> command の一連のrhi層を
// エンドツーエンドで検証する最初のテスト)。

use ash::vk;
use gpu_allocator::MemoryLocation;
use gpu_util::compiled_slang::compile_slang_to_spirv;
use gpu_util::rhi::vulkan::descriptor::DescriptorBindingDesc;
use gpu_util::rhi::vulkan::VulkanDevice;

#[test]
fn dispatches_compute_shader_and_reads_back_buffer() {
    let device = match VulkanDevice::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "Skipping dispatches_compute_shader_and_reads_back_buffer: no Vulkan device ({e:#})"
            );
            return;
        }
    };

    let source = r#"
        [[vk::binding(0, 0)]]
        RWStructuredBuffer<uint> output;

        [shader("compute")]
        [numthreads(64, 1, 1)]
        void main(uint3 tid : SV_DispatchThreadID) {
            output[tid.x] = tid.x * 2;
        }
    "#;

    let spirv = compile_slang_to_spirv(
        "rhi_compute_dispatch_test",
        "rhi_compute_dispatch_test.slang",
        source,
        "main",
        &[],
        &[],
    )
    .expect("shader should compile to SPIR-V");

    const COUNT: u64 = 64;
    let buffer_size = COUNT * std::mem::size_of::<u32>() as u64;

    let buffer = device
        .create_buffer(
            buffer_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
            "compute test output buffer",
        )
        .expect("buffer allocation should succeed");

    let bindings = [DescriptorBindingDesc {
        binding: 0,
        descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
        descriptor_count: 1,
        stage_flags: vk::ShaderStageFlags::COMPUTE,
        variable_count: false,
    }];

    let set_layout = device
        .create_descriptor_set_layout(&bindings)
        .expect("descriptor set layout creation should succeed");
    let pool = device
        .create_descriptor_pool(&bindings, 1)
        .expect("descriptor pool creation should succeed");
    let set = device
        .allocate_descriptor_set(pool, set_layout, None)
        .expect("descriptor set allocation should succeed");

    device.write_buffer_descriptor(
        set,
        0,
        vk::DescriptorType::STORAGE_BUFFER,
        buffer.handle(),
        0,
        buffer_size,
    );

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

    let ptr = buffer
        .mapped_ptr()
        .expect("buffer should be host-mapped")
        .cast::<u32>();
    for i in 0..COUNT as usize {
        let value = unsafe { ptr.as_ptr().add(i).read() };
        assert_eq!(value, (i as u32) * 2, "mismatch at index {i}");
    }

    // 破棄用のRAIIラッパーをまだ用意していない生ハンドル群を手動で解放する。
    unsafe {
        device.device.destroy_command_pool(command_pool, None);
        device.destroy_pipeline(pipeline, layout);
        device.device.destroy_descriptor_pool(pool, None);
        device.device.destroy_descriptor_set_layout(set_layout, None);
    }
}
