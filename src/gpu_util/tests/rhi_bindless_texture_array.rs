// 可変長テクスチャ配列と非uniform添字参照パターンを検証する。
//
// 4枚の1x1テクスチャ(それぞれ異なる値)を1つの可変長Texture2D配列にまとめ、
// 各スレッドがparamsから読んだ(スレッドごとに異なる)添字で
// tex[NonUniformResourceIndex(idx)]を参照して出力バッファへ書き戻す。

use gpu_util::compiled_shader::compile_slang_to_spirv;
use gpu_util::rhi::{
    BindingDesc, BindingKind, BufferUsage, ComputeDispatch, Device, PipelineLayoutDesc, Resource,
    ResourceBinding, SetLayoutDesc, ShaderStages, TextureUsage, TextureView,
};
use gpu_util::rhi::TextureFormat;

const TEXTURE_COUNT: usize = 4;

/// 1x1のRGBA32Floatテクスチャを`value`で塗りつぶして、そのビューを返す。
fn create_filled_texture_view(device: &Device, value: [f32; 4], name: &str) -> TextureView {
    let texture = device
        .create_texture(
            1,
            1,
            TextureFormat::Rgba32Float,
            TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
            name,
        )
        .expect("texture creation should succeed");
    device
        .upload_texture_data(&texture, bytemuck::cast_slice(&value))
        .expect("texture upload should succeed");
    device
        .create_texture_view(&texture)
        .expect("texture view creation should succeed")
}

#[test]
fn reads_variable_length_texture_array_with_nonuniform_indices() {
    let device = match Device::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "Skipping reads_variable_length_texture_array_with_nonuniform_indices: \
                 no GPU device ({e:#})"
            );
            return;
        }
    };

    // 各テクスチャに識別しやすい一意な値を仕込む。
    let views: Vec<TextureView> = (0..TEXTURE_COUNT)
        .map(|i| {
            let v = (i as f32 + 1.0) * 10.0;
            create_filled_texture_view(
                &device,
                [v, v + 1.0, v + 2.0, v + 3.0],
                "bindless test texture",
            )
        })
        .collect();

    // set 0 binding 0 = 可変長テクスチャ配列、set 1 binding 0 = 出力、binding 1 = params。
    let source = r#"
        [[vk::binding(0, 0)]]
        Texture2D<float4> tex[];

        struct Params { uint index; };
        [[vk::binding(1, 1)]]
        StructuredBuffer<Params> params;

        [[vk::binding(0, 1)]]
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

    let binding = |binding, kind, variable_count| BindingDesc {
        binding,
        kind,
        count: 1,
        variable_count,
        stages: ShaderStages::COMPUTE,
    };
    let layout = PipelineLayoutDesc {
        sets: vec![
            SetLayoutDesc {
                bindings: vec![binding(0, BindingKind::SampledImage, true)],
            },
            SetLayoutDesc {
                bindings: vec![
                    binding(0, BindingKind::StorageBuffer, false),
                    binding(1, BindingKind::StorageBuffer, false),
                ],
            },
        ],
    };
    let pipeline = device
        .create_compute_pipeline(&spirv, "main", &layout)
        .expect("compute pipeline creation should succeed");

    // 各スレッドに割り当てる添字を、あえて連番でない(=uniformでない)順序にする。
    let indices: [u32; TEXTURE_COUNT] = [2, 0, 3, 1];
    let params_size = (TEXTURE_COUNT * std::mem::size_of::<u32>()) as u64;
    let params_buffer = device
        .create_buffer(params_size, BufferUsage::STORAGE, true, "params buffer")
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
        .create_buffer(output_size, BufferUsage::STORAGE, true, "output buffer")
        .expect("output buffer creation should succeed");

    device
        .dispatch_compute(ComputeDispatch {
            pipeline: &pipeline,
            sets: &[
                &[ResourceBinding {
                    binding: 0,
                    resource: Resource::TextureArray(&views),
                }],
                &[
                    ResourceBinding {
                        binding: 0,
                        resource: Resource::Buffer(&output_buffer),
                    },
                    ResourceBinding {
                        binding: 1,
                        resource: Resource::Buffer(&params_buffer),
                    },
                ],
            ],
            workgroups: (1, 1, 1),
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
                "thread {thread} (reading texture {texture_index}) channel {c}: expected {expected}, got {got}"
            );
        }
    }
}
