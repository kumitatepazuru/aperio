// compiled_wgsl.rs

use anyhow::{bail, Result};
use naga_oil::compose::{Composer, NagaModuleDescriptor, ShaderDefValue};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::Device;

use crate::compose_wgsl::{ComposableModuleSource, NagaModuleSource};
use crate::image_generator::ImageGenerator;

const IMAGE_FORMAT_ALIAS_SOURCE: &str = include_str!("shaders/image_format.wgsl");

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

/// `output_format` に対応する `ImageStorageTexture` 用の shader_def を1つ返す。
/// `Rgba32Float` は `#else` 側のデフォルト分岐に該当するため定義不要だが、
/// 意図を明示するために同名の(未使用の)defを立てておく。
fn shader_defs_for_format(format: wgpu::TextureFormat) -> Result<HashMap<String, ShaderDefValue>> {
    let def_name = match format {
        wgpu::TextureFormat::Rgba8Unorm => "APERIO_FORMAT_RGBA8UNORM",
        wgpu::TextureFormat::Rgba16Float => "APERIO_FORMAT_RGBA16FLOAT",
        wgpu::TextureFormat::Rgba32Float => "APERIO_FORMAT_RGBA32FLOAT",
        other => bail!(
            "Unsupported output_format for CompiledWgsl: {other:?}. \
             Only Rgba8Unorm/Rgba16Float/Rgba32Float are valid ImageGenerator working formats."
        ),
    };

    let mut defs = HashMap::with_capacity(1);
    defs.insert(def_name.to_string(), ShaderDefValue::Bool(true));
    Ok(defs)
}

#[derive(Clone)]
pub struct CompiledWgsl {
    /// シェーダー種別のラベル。パイプラインキャッシュのキーにもなる
    /// `GenerateStructure.name`と同じ「同種なら共通」の性質を持つ。
    pub(crate) name: String,
    pub(crate) module: Arc<wgpu::ShaderModule>,
    pub(crate) sampler: Option<Arc<wgpu::Sampler>>,
    /// このシェーダーの出力ストレージテクスチャが実際に使うフォーマット
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) _source: Arc<str>,
}

impl CompiledWgsl {
    /// `wgsl_code` を naga_oil で合成する。`#import`を一切使わない単一ファイルで
    /// あっても、内部的には常に naga_oil の合成を通す(`ImageStorageTexture`の
    /// alias定義がソースに自動追記されるため)。
    pub fn new(
        name: &str,
        wgsl_code: &str,
        generator: &ImageGenerator,
        output_format: wgpu::TextureFormat,
        sampler_options: Option<&SamplerOptions>,
    ) -> Result<Self> {
        let naga_module = NagaModuleSource {
            file_path: format!("{name}.wgsl"),
            source: wgsl_code.to_string(),
            shader_defs: HashMap::new(),
        };

        Self::compose_new(name, &[], &naga_module, generator, output_format, sampler_options)
    }

    /// `composable_modules` を naga_oil に登録したうえで `naga_module` を合成し、
    /// できあがった naga IR からシェーダーモジュールを作る。
    /// `ImageStorageTexture`(出力ストレージテクスチャ型のエイリアス)は呼び出し側が
    /// 意識しなくてよいよう、`naga_module`のソースに自動的に追記される。
    ///
    /// `name` はパイプラインキャッシュのキーとしても使われるため、
    /// 同じシェーダーを異なる `shader_defs` で合成する場合は必ず別の `name` を渡すこと。
    pub fn compose_new(
        name: &str,
        composable_modules: &[&ComposableModuleSource],
        naga_module: &NagaModuleSource,
        generator: &ImageGenerator,
        output_format: wgpu::TextureFormat,
        sampler_options: Option<&SamplerOptions>,
    ) -> Result<Self> {
        let device = &generator.device;
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

        // `ImageStorageTexture`のalias定義をエントリソースの末尾に追記し、
        // その解決に必要なshader_defsを(呼び出し元が指定したものに)マージする。
        let augmented_source = format!("{}\n{IMAGE_FORMAT_ALIAS_SOURCE}", naga_module.source);
        let mut shader_defs = naga_module.shader_defs.clone();
        shader_defs.extend(shader_defs_for_format(output_format)?);

        let naga_module_descriptor = NagaModuleDescriptor {
            source: &augmented_source,
            file_path: &naga_module.file_path,
            shader_defs,
            ..Default::default()
        };

        let module = match composer.make_naga_module(naga_module_descriptor) {
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
            format: output_format,
            // naga IR から作るため WGSL のテキストは持たない
            _source: Arc::from(""),
        })
    }
}
