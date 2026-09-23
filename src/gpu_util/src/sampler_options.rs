// シェーダーコンパイル経路(Slang/HLSL等)に共通のサンプラー設定。
// 特定のシェーダー言語には依存しない。

use std::sync::Arc;
use wgpu::Device;

pub struct SamplerOptions {
    pub address_mode: wgpu::AddressMode,
    pub filter: wgpu::FilterMode,
}

/// wgpu 30 で mipmap_filter だけ別の enum になったため変換する。
fn mipmap_filter_of(filter: wgpu::FilterMode) -> wgpu::MipmapFilterMode {
    match filter {
        wgpu::FilterMode::Nearest => wgpu::MipmapFilterMode::Nearest,
        wgpu::FilterMode::Linear => wgpu::MipmapFilterMode::Linear,
    }
}

pub fn build_sampler(
    device: &Device,
    sampler_options: Option<&SamplerOptions>,
) -> Option<Arc<wgpu::Sampler>> {
    sampler_options.map(|options| {
        Arc::new(device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: options.address_mode,
            address_mode_v: options.address_mode,
            address_mode_w: options.address_mode,
            mag_filter: options.filter,
            min_filter: options.filter,
            mipmap_filter: mipmap_filter_of(options.filter),
            ..Default::default()
        }))
    })
}
