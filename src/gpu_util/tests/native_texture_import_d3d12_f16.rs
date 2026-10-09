// Windows: D3D12が作った共有テクスチャ(RGBA16F)へ、Vulkanが作業テクスチャ(RGBA8)を書き込む。
//
// 作業フォーマット(8bit)→共有フォーマット(half float)の変換と、値の範囲を確認する。
#![cfg(target_os = "windows")]

mod support;

use gpu_util::image_generator::ImageGenerator;
use gpu_util::rhi::{TextureFormat, TextureUsage};
use gpu_util::texture_to_native::windows::{attach_texture_to_shared_texture, SharedTextureHandle};
use gpu_util::{ImagePixelFormat, SharedTextureFormat};
use support::SharedTarget;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R16G16B16A16_FLOAT;

const SIZE: u32 = 4;

#[test]
fn vulkan_writes_into_d3d12_shared_f16_texture() {
    let generator = match ImageGenerator::new(ImagePixelFormat::Rgba8Unorm) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Skipping vulkan_writes_into_d3d12_shared_f16_texture: no GPU ({e:#})");
            return;
        }
    };
    let Some(luid) = generator.device.adapter_luid() else {
        eprintln!("Skipping: the Vulkan device does not report an adapter LUID");
        return;
    };

    // ピクセル(x, y) = (x*64, y*64, 255, 255) の8bit値
    let mut pixels = Vec::<u8>::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            pixels.extend_from_slice(&[(x * 64).min(255) as u8, (y * 64).min(255) as u8, 255, 255]);
        }
    }
    let source = generator
        .device
        .create_texture(
            SIZE,
            SIZE,
            TextureFormat::Rgba8Unorm,
            TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
            "source",
        )
        .unwrap();
    generator.device.upload_texture_data(&source, &pixels).unwrap();

    let target = SharedTarget::new(luid, SIZE, SIZE, DXGI_FORMAT_R16G16B16A16_FLOAT, 8)
        .expect("D3D12 shared texture creation");

    if let Err(e) = attach_texture_to_shared_texture(
        &SharedTextureHandle {
            nt_handle: target.handle_bytes(),
        },
        &SharedTextureFormat::Rgba16Float,
        &source,
        &generator,
    ) {
        eprintln!("Skipping: D3D12 import is unavailable on this device ({e:#})");
        return;
    }

    let out = target.read_back().expect("D3D12 readback");
    assert_eq!(out.len(), (SIZE * SIZE * 8) as usize);
    for y in 0..SIZE as usize {
        for x in 0..SIZE as usize {
            let i = (y * SIZE as usize + x) * 8;
            let channel = |c: usize| half::f16::from_le_bytes([out[i + c * 2], out[i + c * 2 + 1]]).to_f32();
            let want = [
                (x as f32 * 64.0).min(255.0) / 255.0,
                (y as f32 * 64.0).min(255.0) / 255.0,
                1.0,
                1.0,
            ];
            for c in 0..4 {
                assert!(
                    (channel(c) - want[c]).abs() < 0.003,
                    "pixel ({x}, {y}) channel {c}: expected {}, got {}",
                    want[c],
                    channel(c)
                );
            }
        }
    }
}
