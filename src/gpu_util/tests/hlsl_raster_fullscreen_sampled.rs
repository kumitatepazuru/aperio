// HLSL(DXC)で書いたラスターシェーダーをRHIのグラフィックスパイプラインで実行する。
//
// D3D向けに書かれたフルスクリーン三角形(NDCのy+が上)が、Slangの場合と同様に
// 上下反転せず写ることと、DXCがエントリポイント名をそのまま残すこと
// (RHIにも"vs_main" / "fs_main"をそのまま渡す)を確認する。

use gpu_util::compiled_hlsl::{compile_hlsl_to_spirv, HlslStage};
use gpu_util::rhi::{
    AddressMode, BindingDesc, BindingKind, Device, Draw, FilterMode, GraphicsPipelineDesc,
    PipelineLayoutDesc, Resource, ResourceBinding, SamplerOptions, SetLayoutDesc, ShaderStages,
    TextureFormat, TextureUsage,
};

const SOURCE: &str = r#"
    [[vk::binding(0, 0)]] Texture2D<float4> tex;
    [[vk::binding(1, 0)]] SamplerState samp;

    struct VsOut {
        float4 pos : SV_Position;
        [[vk::location(0)]] float2 uv : TEXCOORD0;
    };

    VsOut vs_main(uint vid : SV_VertexID) {
        float2 p = float2((vid << 1) & 2, vid & 2);
        VsOut o;
        o.pos = float4(p * 2.0 - 1.0, 0.0, 1.0);
        o.uv = float2(p.x, 1.0 - p.y);
        return o;
    }

    float4 fs_main(VsOut i) : SV_Target {
        return tex.Sample(samp, i.uv);
    }
"#;

#[test]
fn draws_hlsl_fullscreen_triangle_with_sampled_texture() {
    let device = match Device::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping draws_hlsl_fullscreen_triangle_with_sampled_texture: no GPU ({e:#})");
            return;
        }
    };

    let vs = compile_hlsl_to_spirv("fullscreen.hlsl", SOURCE, "vs_main", HlslStage::Vertex, &[], &[])
        .expect("vertex shader should compile");
    let fs = compile_hlsl_to_spirv("fullscreen.hlsl", SOURCE, "fs_main", HlslStage::Fragment, &[], &[])
        .expect("fragment shader should compile");

    let binding = |binding, kind| BindingDesc {
        binding,
        kind,
        count: 1,
        variable_count: false,
        stages: ShaderStages::FRAGMENT,
    };
    let pipeline = device
        .create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex_spirv: &vs,
            vertex_entry: "vs_main",
            fragment_spirv: &fs,
            fragment_entry: "fs_main",
            layout: PipelineLayoutDesc {
                sets: vec![SetLayoutDesc {
                    bindings: vec![
                        binding(0, BindingKind::SampledImage),
                        binding(1, BindingKind::Sampler),
                    ],
                }],
            },
            vertex_buffers: vec![],
            color_format: TextureFormat::Rgba8Unorm,
            blend: None,
        })
        .expect("graphics pipeline should be created");

    // 左上=赤、右上=緑、左下=青、右下=白。
    let pixels: [u8; 16] = [
        255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let source = device
        .create_texture(
            2,
            2,
            TextureFormat::Rgba8Unorm,
            TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
            "source",
        )
        .unwrap();
    device.upload_texture_data(&source, &pixels).unwrap();
    let source_view = device.create_texture_view(&source).unwrap();
    let target = device
        .create_texture(
            2,
            2,
            TextureFormat::Rgba8Unorm,
            TextureUsage::COLOR_TARGET | TextureUsage::TRANSFER_SRC,
            "target",
        )
        .unwrap();
    let target_view = device.create_texture_view(&target).unwrap();
    let sampler = device
        .create_sampler(&SamplerOptions {
            address_mode: AddressMode::ClampToEdge,
            filter: FilterMode::Nearest,
        })
        .unwrap();

    device
        .render_pass(
            &target_view,
            Some([0.0; 4]),
            &[Draw {
                pipeline: &pipeline,
                sets: &[&[
                    ResourceBinding {
                        binding: 0,
                        resource: Resource::Texture(&source_view),
                    },
                    ResourceBinding {
                        binding: 1,
                        resource: Resource::Sampler(&sampler),
                    },
                ]],
                vertex_buffers: &[],
                vertex_count: 3,
                instance_count: 1,
            }],
        )
        .expect("render pass should succeed");

    assert_eq!(
        device.download_texture_data(&target).unwrap(),
        pixels,
        "target should reproduce the source unflipped"
    );
}
