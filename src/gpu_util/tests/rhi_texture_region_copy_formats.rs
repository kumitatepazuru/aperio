// 矩形アップロード・テクスチャ間コピー・追加フォーマットの往復を検証する。

use gpu_util::rhi::{Device, TextureCopy, TextureFormat, TextureUsage};

fn device_or_skip(name: &str) -> Option<Device> {
    match Device::new() {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("Skipping {name}: no GPU ({e:#})");
            None
        }
    }
}

fn usage() -> TextureUsage {
    TextureUsage::SAMPLED | TextureUsage::TRANSFER_SRC | TextureUsage::TRANSFER_DST
}

#[test]
fn uploads_region_and_copies_between_textures() {
    let Some(device) = device_or_skip("uploads_region_and_copies_between_textures") else {
        return;
    };

    let src = device
        .create_texture(4, 4, TextureFormat::R8Unorm, usage(), "src")
        .unwrap();
    let dst = device
        .create_texture(4, 4, TextureFormat::R8Unorm, usage(), "dst")
        .unwrap();
    device.upload_texture_data(&src, &[0u8; 16]).unwrap();
    device.upload_texture_data(&dst, &[7u8; 16]).unwrap();

    // src の (1,1) から 2x2 に値を書き込む。
    device
        .upload_texture_region(&src, (1, 1), (2, 2), &[10, 20, 30, 40])
        .unwrap();
    let src_data = device.download_texture_data(&src).unwrap();
    #[rustfmt::skip]
    assert_eq!(src_data, [
        0,  0,  0, 0,
        0, 10, 20, 0,
        0, 30, 40, 0,
        0,  0,  0, 0,
    ]);

    // src の (1,1) 2x2 を dst の (2,0) へコピーする。
    device
        .copy_textures(&[TextureCopy {
            src: &src,
            src_origin: (1, 1),
            dst: &dst,
            dst_origin: (2, 0),
            size: (2, 2),
        }])
        .unwrap();
    let dst_data = device.download_texture_data(&dst).unwrap();
    #[rustfmt::skip]
    assert_eq!(dst_data, [
        7, 7, 10, 20,
        7, 7, 30, 40,
        7, 7,  7,  7,
        7, 7,  7,  7,
    ]);
}

#[test]
fn round_trips_single_and_dual_channel_formats() {
    let Some(device) = device_or_skip("round_trips_single_and_dual_channel_formats") else {
        return;
    };

    for format in [
        TextureFormat::R8Unorm,
        TextureFormat::Rg8Unorm,
        TextureFormat::R16Unorm,
        TextureFormat::Rg16Unorm,
    ] {
        let texture = match device.create_texture(3, 2, format, usage(), "format test") {
            Ok(t) => t,
            Err(e) => {
                // 16bit正規化形式は任意機能なので、非対応のデバイスではスキップする。
                eprintln!("Skipping {format:?}: {e:#}");
                continue;
            }
        };
        let len = 3 * 2 * format.bytes_per_texel() as usize;
        let data: Vec<u8> = (0..len).map(|i| (i * 7 + 1) as u8).collect();
        device.upload_texture_data(&texture, &data).unwrap();
        assert_eq!(
            device.download_texture_data(&texture).unwrap(),
            data,
            "{format:?} should round-trip"
        );
    }
}
