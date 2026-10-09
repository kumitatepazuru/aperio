// aperioのコンピュートシェーダー規約をRHIのレイアウト記述へ落とし込む。
//
//   [vk::binding(0, 0)] ParameterBlock<...> inputs;  // set 0: 入力テクスチャ
//   [vk::binding(0, 1)] ParameterBlock<...> res;     // set 1: 出力 -> (サンプラー) -> (params)
//
// RHIはレイアウトを受け取って処理するだけで、この規約はImageGenerator側でのみ持つ。

use anyhow::Result;

use crate::rhi::{
    BindingDesc, BindingKind, Buffer, ComputeDispatch, ComputePipeline, Device, PipelineLayoutDesc,
    Resource, ResourceBinding, Sampler, SetLayoutDesc, ShaderStages, TextureView,
};

/// 入力テクスチャの束ね方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputArity {
    /// tex0, tex1, ... のように1枚ずつ別バインディング(本数はシェーダーごとに固定)。
    Fixed,
    /// Texture2D<float4> tex[]のような単一の可変長配列バインディング。
    Variable,
}

/// 出力リソースの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Texture,
    Buffer,
}

/// シェーダーのリソース形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShaderShape {
    pub input_arity: InputArity,
    /// input_arity == Fixedの場合の入力本数。Variableでは無視される。
    pub input_count: u32,
    pub has_sampler: bool,
    pub has_params: bool,
    pub output: OutputKind,
}

impl ShaderShape {
    pub fn layout(&self) -> PipelineLayoutDesc {
        let binding = |binding, kind| BindingDesc {
            binding,
            kind,
            count: 1,
            variable_count: false,
            stages: ShaderStages::COMPUTE,
        };

        let inputs = match self.input_arity {
            InputArity::Fixed => (0..self.input_count)
                .map(|i| binding(i, BindingKind::SampledImage))
                .collect(),
            InputArity::Variable => vec![BindingDesc {
                variable_count: true,
                ..binding(0, BindingKind::SampledImage)
            }],
        };

        let mut res = vec![binding(
            0,
            match self.output {
                OutputKind::Texture => BindingKind::StorageImage,
                OutputKind::Buffer => BindingKind::StorageBuffer,
            },
        )];
        if self.has_sampler {
            res.push(binding(res.len() as u32, BindingKind::Sampler));
        }
        if self.has_params {
            res.push(binding(res.len() as u32, BindingKind::StorageBuffer));
        }

        PipelineLayoutDesc {
            sets: vec![
                SetLayoutDesc { bindings: inputs },
                SetLayoutDesc { bindings: res },
            ],
        }
    }
}

/// `dispatch`の出力先。
pub enum DispatchOutput<'a> {
    Texture(&'a TextureView),
    Buffer(&'a Buffer),
}

/// aperio規約に沿ったコンピュートディスパッチ。形状はパイプラインと一致させる
/// (食い違いはRHIのレイアウト検証でエラーになる)。
pub struct AperioDispatch<'a> {
    pub pipeline: &'a ComputePipeline,
    pub inputs: &'a [TextureView],
    pub output: DispatchOutput<'a>,
    pub sampler: Option<&'a Sampler>,
    pub params: Option<&'a Buffer>,
    pub workgroups: (u32, u32, u32),
}

pub fn dispatch(device: &Device, d: AperioDispatch<'_>) -> Result<()> {
    let AperioDispatch {
        pipeline,
        inputs,
        output,
        sampler,
        params,
        workgroups,
    } = d;

    // 入力側: 可変長ならbinding 0に配列、固定長なら1枚ずつ。
    // どちらの形かはパイプラインのレイアウトから判定する。
    let variable = pipeline.layout().sets.first().is_some_and(|set| {
        set.bindings.iter().any(|b| b.variable_count)
    });
    let input_bindings: Vec<ResourceBinding<'_>> = if variable {
        vec![ResourceBinding {
            binding: 0,
            resource: Resource::TextureArray(inputs),
        }]
    } else {
        inputs
            .iter()
            .enumerate()
            .map(|(i, view)| ResourceBinding {
                binding: i as u32,
                resource: Resource::Texture(view),
            })
            .collect()
    };

    let mut res_bindings = vec![ResourceBinding {
        binding: 0,
        resource: match output {
            DispatchOutput::Texture(view) => Resource::Texture(view),
            DispatchOutput::Buffer(buffer) => Resource::Buffer(buffer),
        },
    }];
    if let Some(sampler) = sampler {
        res_bindings.push(ResourceBinding {
            binding: res_bindings.len() as u32,
            resource: Resource::Sampler(sampler),
        });
    }
    if let Some(params) = params {
        res_bindings.push(ResourceBinding {
            binding: res_bindings.len() as u32,
            resource: Resource::Buffer(params),
        });
    }

    device.dispatch_compute(ComputeDispatch {
        pipeline,
        sets: &[&input_bindings, &res_bindings],
        workgroups,
    })
}
