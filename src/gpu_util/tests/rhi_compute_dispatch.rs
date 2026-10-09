// Slangでコンパイルしたコンピュートシェーダーをパイプライン化し、出力バッファへ
// dispatchして結果をCPUから読み戻せることを確認する。

use gpu_util::compiled_shader::compile_slang_to_spirv;
use gpu_util::rhi::{
    BindingDesc, BindingKind, BufferUsage, ComputeDispatch, Device, PipelineLayoutDesc, Resource,
    ResourceBinding, SetLayoutDesc, ShaderStages,
};

#[test]
fn dispatches_compute_shader_and_reads_back_buffer() {
    let device = match Device::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "Skipping dispatches_compute_shader_and_reads_back_buffer: no GPU device ({e:#})"
            );
            return;
        }
    };

    // RHIは任意のレイアウトを受け取れる。ここではset 1 binding 0 = 出力バッファのみ。
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

    let layout = PipelineLayoutDesc {
        sets: vec![SetLayoutDesc {
            bindings: vec![BindingDesc {
                binding: 0,
                kind: BindingKind::StorageBuffer,
                count: 1,
                variable_count: false,
                stages: ShaderStages::COMPUTE,
            }],
        }],
    };
    let pipeline = device
        .create_compute_pipeline(&spirv, "main", &layout)
        .expect("compute pipeline creation should succeed");

    const COUNT: u64 = 64;
    let buffer = device
        .create_buffer(
            COUNT * 4,
            BufferUsage::STORAGE,
            true,
            "compute test output buffer",
        )
        .expect("buffer allocation should succeed");

    device
        .dispatch_compute(ComputeDispatch {
            pipeline: &pipeline,
            sets: &[&[ResourceBinding {
                binding: 0,
                resource: Resource::Buffer(&buffer),
            }]],
            workgroups: (1, 1, 1),
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
}
