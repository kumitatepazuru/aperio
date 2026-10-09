// 頂点バッファ(頂点ごと + インスタンスごと)とアルファブレンドを使うインスタンス描画。
//
// 4x4の青いターゲットへ1ピクセル大の矩形を2つ描く。
//   - インスタンス0: 左上ピクセルに不透明な赤
//   - インスタンス1: 右下ピクセルに半透明の緑(SrcAlpha / OneMinusSrcAlpha)

use gpu_util::compiled_shader::compile_slang_to_spirv;
use gpu_util::rhi::{
    BlendComponent, BlendFactor, BlendState, BufferUsage, Device, Draw, GraphicsPipelineDesc,
    PipelineLayoutDesc, TextureFormat, TextureUsage, VertexAttribute, VertexBufferLayout,
    VertexFormat,
};

const SOURCE: &str = r#"
    struct VsIn {
        [[vk::location(0)]] float2 corner : CORNER;
        [[vk::location(1)]] float2 origin : ORIGIN;
        [[vk::location(2)]] float4 color : COLOR;
    };

    struct VsOut {
        float4 pos : SV_Position;
        [[vk::location(0)]] float4 color : COLOR;
    };

    [shader("vertex")]
    VsOut vs_main(VsIn i) {
        VsOut o;
        // 4x4ターゲットではNDC 0.5 = 1ピクセル。
        o.pos = float4(i.origin + i.corner * 0.5, 0.0, 1.0);
        o.color = i.color;
        return o;
    }

    [shader("fragment")]
    float4 fs_main(VsOut i) : SV_Target {
        return i.color;
    }
"#;

fn pixel(data: &[u8], x: usize, y: usize) -> [u8; 4] {
    let i = (y * 4 + x) * 4;
    data[i..i + 4].try_into().unwrap()
}

fn assert_close(got: [u8; 4], expected: [u8; 4], what: &str) {
    for c in 0..4 {
        assert!(
            got[c].abs_diff(expected[c]) <= 2,
            "{what}: expected {expected:?}, got {got:?}"
        );
    }
}

#[test]
fn draws_instanced_quads_with_alpha_blend() {
    let device = match Device::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping draws_instanced_quads_with_alpha_blend: no GPU ({e:#})");
            return;
        }
    };

    let compile = |entry| {
        compile_slang_to_spirv(
            "rhi_raster_instanced",
            "rhi_raster_instanced.slang",
            SOURCE,
            entry,
            &[],
            &[],
        )
        .expect("shader should compile")
    };
    let (vs, fs) = (compile("vs_main"), compile("fs_main"));

    let blend = BlendComponent {
        src_factor: BlendFactor::SrcAlpha,
        dst_factor: BlendFactor::OneMinusSrcAlpha,
    };
    let pipeline = device
        .create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex_spirv: &vs,
            vertex_entry: "main",
            fragment_spirv: &fs,
            fragment_entry: "main",
            layout: PipelineLayoutDesc::default(),
            vertex_buffers: vec![
                VertexBufferLayout {
                    stride: 8,
                    per_instance: false,
                    attributes: vec![VertexAttribute {
                        location: 0,
                        format: VertexFormat::Float32x2,
                        offset: 0,
                    }],
                },
                VertexBufferLayout {
                    stride: 24,
                    per_instance: true,
                    attributes: vec![
                        VertexAttribute {
                            location: 1,
                            format: VertexFormat::Float32x2,
                            offset: 0,
                        },
                        VertexAttribute {
                            location: 2,
                            format: VertexFormat::Float32x4,
                            offset: 8,
                        },
                    ],
                },
            ],
            color_format: TextureFormat::Rgba8Unorm,
            blend: Some(BlendState {
                color: blend,
                alpha: BlendComponent {
                    src_factor: BlendFactor::One,
                    dst_factor: BlendFactor::OneMinusSrcAlpha,
                },
            }),
        })
        .expect("graphics pipeline should be created");

    let corners: [f32; 12] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0];
    let instances: [f32; 12] = [
        -1.0, 0.5, 1.0, 0.0, 0.0, 1.0, // 左上ピクセル、不透明な赤
        0.5, -1.0, 0.0, 1.0, 0.0, 0.5, // 右下ピクセル、半透明の緑
    ];
    let upload = |data: &[f32], label| {
        let buffer = device
            .create_buffer(
                std::mem::size_of_val(data) as u64,
                BufferUsage::VERTEX,
                true,
                label,
            )
            .unwrap();
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr() as *const u8,
                buffer.mapped_ptr().unwrap().as_ptr().cast::<u8>(),
                std::mem::size_of_val(data),
            );
        }
        buffer
    };
    let corner_buffer = upload(&corners, "corners");
    let instance_buffer = upload(&instances, "instances");

    let target = device
        .create_texture(
            4,
            4,
            TextureFormat::Rgba8Unorm,
            TextureUsage::COLOR_TARGET | TextureUsage::TRANSFER_SRC,
            "target",
        )
        .unwrap();
    let target_view = device.create_texture_view(&target).unwrap();

    device
        .render_pass(
            &target_view,
            Some([0.0, 0.0, 1.0, 1.0]),
            &[Draw {
                pipeline: &pipeline,
                sets: &[],
                vertex_buffers: &[&corner_buffer, &instance_buffer],
                vertex_count: 6,
                instance_count: 2,
            }],
        )
        .expect("render pass should succeed");

    let out = device.download_texture_data(&target).unwrap();
    assert_close(pixel(&out, 0, 0), [255, 0, 0, 255], "top-left (opaque red)");
    assert_close(pixel(&out, 3, 3), [0, 128, 128, 255], "bottom-right (green over blue)");
    assert_close(pixel(&out, 1, 1), [0, 0, 255, 255], "untouched (clear color)");
    assert_close(pixel(&out, 3, 0), [0, 0, 255, 255], "top-right untouched");
    assert_close(pixel(&out, 0, 3), [0, 0, 255, 255], "bottom-left untouched");
}
