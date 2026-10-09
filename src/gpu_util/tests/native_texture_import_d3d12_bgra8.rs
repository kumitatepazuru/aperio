// Windows: D3D12が作った共有テクスチャ(BGRA8)へ、Vulkanが作業テクスチャ(RGBA32F)を書き込む。
//
// D3D12側は共有ハンドルを提供するだけで、書き込みはVulkanが行う。結果をD3D12側で
// 読み戻し、色・チャンネル順(BGRA)・上下の向きが保たれていることを確認する。
#![cfg(target_os = "windows")]

mod support;

use gpu_util::image_generator::ImageGenerator;
use gpu_util::rhi::{TextureFormat, TextureUsage};
use gpu_util::texture_to_native::windows::{attach_texture_to_shared_texture, SharedTextureHandle};
use gpu_util::{ImagePixelFormat, SharedTextureFormat};
use support::SharedTarget;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

const SIZE: u32 = 8;

#[test]
fn vulkan_writes_into_d3d12_shared_bgra8_texture() {
    let generator = match ImageGenerator::new(ImagePixelFormat::Rgba32Float) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Skipping vulkan_writes_into_d3d12_shared_bgra8_texture: no GPU ({e:#})");
            return;
        }
    };
    let Some(luid) = generator.device.adapter_luid() else {
        eprintln!("Skipping: the Vulkan device does not report an adapter LUID");
        return;
    };

    // ピクセル(x, y) = (x/7, y/7, 0.5, 1.0)
    let mut pixels = Vec::<f32>::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            pixels.extend_from_slice(&[x as f32 / 7.0, y as f32 / 7.0, 0.5, 1.0]);
        }
    }
    let source = generator
        .device
        .create_texture(
            SIZE,
            SIZE,
            TextureFormat::Rgba32Float,
            TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
            "source",
        )
        .unwrap();
    generator
        .device
        .upload_texture_data(&source, bytemuck::cast_slice(&pixels))
        .unwrap();

    let target = SharedTarget::new(luid, SIZE, SIZE, DXGI_FORMAT_B8G8R8A8_UNORM, 4)
        .expect("D3D12 shared texture creation");

    if let Err(e) = attach_texture_to_shared_texture(
        &SharedTextureHandle {
            nt_handle: target.handle_bytes(),
        },
        &SharedTextureFormat::Bgra8Unorm,
        &source,
        &generator,
    ) {
        // 外部メモリ拡張が無いドライバではインポートできない。
        eprintln!("Skipping: D3D12 import is unavailable on this device ({e:#})");
        return;
    }

    let out = target.read_back().expect("D3D12 readback");
    assert_eq!(out.len(), (SIZE * SIZE * 4) as usize);
    for y in 0..SIZE as usize {
        for x in 0..SIZE as usize {
            let i = (y * SIZE as usize + x) * 4;
            let (b, g, r, a) = (out[i], out[i + 1], out[i + 2], out[i + 3]);
            let expect = |v: f32| (v * 255.0).round() as u8;
            let want = (expect(0.5), expect(y as f32 / 7.0), expect(x as f32 / 7.0), 255);
            for (got, want, name) in [
                (b, want.0, "B"),
                (g, want.1, "G"),
                (r, want.2, "R"),
                (a, want.3, "A"),
            ] {
                assert!(
                    got.abs_diff(want) <= 1,
                    "pixel ({x}, {y}) {name}: expected {want}, got {got}"
                );
            }
        }
    }
}
