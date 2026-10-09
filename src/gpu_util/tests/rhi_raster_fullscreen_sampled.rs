// フルスクリーン三角形でテクスチャをサンプリングして描画する最小のラスターパス。
//
// 2x2のソースを2x2のターゲットへ最近傍で描き、ピクセルが上下反転せず
// そのまま写ることを確認する(wgpuと同じNDC向きの検証を兼ねる)。

use gpu_util::compiled_shader::compile_slang_to_spirv;
use gpu_util::rhi::{
    AddressMode, BindingDesc, BindingKind, Device, Draw, FilterMode, GraphicsPipelineDesc,
    PipelineLayoutDesc, Resource, ResourceBinding, SamplerOptions, SetLayoutDesc, ShaderStages,
    TextureFormat, TextureUsage,
};

const SOURCE: &str = r#"
    [[vk::binding(0, 0)]] Texture2D<float4> tex;
    [[vk::binding(0, 1)]] SamplerState samp;

    struct VsOut {
        float4 pos : SV_Position;
        [[vk::location(0)]] float2 uv : TEXCOORD0;
    };

    [shader("vertex")]
    VsOut vs_main(uint vid : SV_VertexID) {
        float2 p = float2((vid << 1) & 2, vid & 2);
        VsOut o;
        o.pos = float4(p * 2.0 - 1.0, 0.0, 1.0);
        o.uv = float2(p.x, 1.0 - p.y);
        return o;
    }

    [shader("fragment")]
    float4 fs_main(VsOut i) : SV_Target {
        return tex.Sample(samp, i.uv);
    }
"#;

#[test]
fn draws_fullscreen_triangle_with_sampled_texture() {
    let device = match Device::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping draws_fullscreen_triangle_with_sampled_texture: no GPU ({e:#})");
            return;
        }
    };

    let compile = |entry| {
        compile_slang_to_spirv(
            "rhi_raster_fullscreen",
            "rhi_raster_fullscreen.slang",
            SOURCE,
            entry,
            &[],
            &[],
        )
        .expect("shader should compile")
    };
    let (vs, fs) = (compile("vs_main"), compile("fs_main"));

    let binding = |binding, kind| BindingDesc {
        binding,
        kind,
        count: 1,
        variable_count: false,
        stages: ShaderStages::FRAGMENT,
    };
    let layout = PipelineLayoutDesc {
        sets: vec![
            SetLayoutDesc {
                bindings: vec![binding(0, BindingKind::SampledImage)],
            },
            SetLayoutDesc {
                bindings: vec![binding(0, BindingKind::Sampler)],
            },
        ],
    };
    let pipeline = device
        .create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex_spirv: &vs,
            vertex_entry: "main",
            fragment_spirv: &fs,
            fragment_entry: "main",
            layout,
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
            Some([0.0, 0.0, 0.0, 0.0]),
            &[Draw {
                pipeline: &pipeline,
                sets: &[
                    &[ResourceBinding {
                        binding: 0,
                        resource: Resource::Texture(&source_view),
                    }],
                    &[ResourceBinding {
                        binding: 0,
                        resource: Resource::Sampler(&sampler),
                    }],
                ],
                vertex_buffers: &[],
                vertex_count: 3,
                instance_count: 1,
            }],
        )
        .expect("render pass should succeed");

    let out = device.download_texture_data(&target).unwrap();
    assert_eq!(out, pixels, "target should reproduce the source unflipped");
}
