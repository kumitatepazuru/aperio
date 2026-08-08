// compiled_wgsl.rs

use anyhow::{bail, Result};
use naga_oil::compose::Composer;
use std::sync::Arc;
use wgpu::Device;

use crate::compose_wgsl::{ComposableModuleSource, NagaModuleSource};

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

fn build_sampler(
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

#[derive(Clone)]
pub struct CompiledWgsl {
    /// シェーダー種別のラベル。パイプラインキャッシュのキーにもなる
    /// `GenerateStructure.name`と同じ「同種なら共通」の性質を持つ。
    pub(crate) name: String,
    pub(crate) module: Arc<wgpu::ShaderModule>,
    pub(crate) sampler: Option<Arc<wgpu::Sampler>>,
    pub(crate) _source: Arc<str>,
}

impl CompiledWgsl {
    pub fn new(
        name: &str,
        wgsl_code: &str,
        device: &Device,
        sampler_options: Option<&SamplerOptions>,
    ) -> Result<Self> {
        let shader_module_descriptor = wgpu::ShaderModuleDescriptor {
            label: Some(name), // labelにもnameを使用
            source: wgpu::ShaderSource::Wgsl(wgsl_code.into()),
        };

        let module = device.create_shader_module(shader_module_descriptor);

        Ok(Self {
            name: name.to_string(),
            module: Arc::new(module),
            sampler: build_sampler(device, sampler_options),
            _source: Arc::from(wgsl_code),
        })
    }

    /// `composable_modules` を naga_oil に登録したうえで `naga_module` を合成し、
    /// できあがった naga IR からシェーダーモジュールを作る。
    ///
    /// `name` はパイプラインキャッシュのキーとしても使われるため、
    /// 同じシェーダーを異なる `shader_defs` で合成する場合は必ず別の `name` を渡すこと。
    pub fn compose_new(
        name: &str,
        composable_modules: &[&ComposableModuleSource],
        naga_module: &NagaModuleSource,
        device: &Device,
        sampler_options: Option<&SamplerOptions>,
    ) -> Result<Self> {
        let mut composer = Composer::non_validating(); // validateをかけようとするとwgpu固有の機能を多く使用しており、保守コストが増えて原因不明のバグの温床になるため、naga_oilのvalidateは使用しない。

        for composable in composable_modules {
            // Ok 側が composer を借用したままになるので、一度 () に落として借用を切ってから
            // emit_to_string(&composer) を呼ぶ
            let result = composer
                .add_composable_module(composable.descriptor())
                .map(|_| ());
            if let Err(e) = result {
                let msg = e.emit_to_string(&composer);
                bail!(
                    "Failed to add composable module {}: {msg}",
                    composable.file_path
                );
            }
        }

        let module = match composer.make_naga_module(naga_module.descriptor()) {
            Ok(module) => module,
            Err(e) => {
                let msg = e.emit_to_string(&composer);
                bail!(
                    "Failed to compose naga module {}: {msg}",
                    naga_module.file_path
                );
            }
        };

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(name), // labelにもnameを使用
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
        });

        Ok(Self {
            name: name.to_string(),
            module: Arc::new(shader_module),
            sampler: build_sampler(device, sampler_options),
            // naga IR から作るため WGSL のテキストは持たない
            _source: Arc::from(""),
        })
    }
}
