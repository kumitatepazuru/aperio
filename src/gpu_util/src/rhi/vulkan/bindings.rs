// ディスクリプタセットへのリソース束縛(compute / graphics 共通)。

use std::sync::Arc;

use anyhow::{bail, Result};
use ash::vk;

use super::descriptor::DescriptorSetLayout;
use super::device::VulkanDevice;
use super::resources::{Buffer, Sampler, Texture, TextureView};
use crate::rhi::{BindingKind, PipelineLayoutDesc};

/// 1バインディングに束縛するリソース(rhi::Resourceのバックエンド側表現)。
pub enum Resource<'a> {
    Texture(&'a TextureView),
    TextureArray(Vec<&'a TextureView>),
    Buffer(&'a Buffer),
    Sampler(&'a Sampler),
}

pub struct ResourceBinding<'a> {
    pub binding: u32,
    pub resource: Resource<'a>,
}

/// コマンド記録時にレイアウト遷移させる必要があるテクスチャ。
#[derive(Default)]
pub(super) struct TouchedTextures<'a> {
    pub sampled: Vec<&'a Arc<Texture>>,
    pub storage: Vec<&'a Arc<Texture>>,
}

impl VulkanDevice {
    /// リソースがレイアウトに過不足なく一致していることを検証し、セットごとの
    /// 可変長バインディングの実際の本数を返す。
    pub(super) fn validate_sets(
        &self,
        layout_desc: &PipelineLayoutDesc,
        sets: &[Vec<ResourceBinding<'_>>],
    ) -> Result<Vec<u32>> {
        if sets.len() != layout_desc.sets.len() {
            bail!(
                "pipeline has {} descriptor set(s), got resources for {}",
                layout_desc.sets.len(),
                sets.len()
            );
        }

        let mut variable_counts = Vec::with_capacity(sets.len());
        for (set_index, (set_desc, resources)) in layout_desc.sets.iter().zip(sets).enumerate() {
            if let Some(extra) = resources
                .iter()
                .find(|r| !set_desc.bindings.iter().any(|b| b.binding == r.binding))
            {
                bail!(
                    "set {set_index} has no binding {} in the pipeline layout",
                    extra.binding
                );
            }

            let mut variable_count = 0;
            for desc in &set_desc.bindings {
                let mut matches = resources.iter().filter(|r| r.binding == desc.binding);
                let (Some(res), None) = (matches.next(), matches.next()) else {
                    bail!(
                        "set {set_index} binding {} must be given exactly one resource",
                        desc.binding
                    );
                };

                let texture_len = match &res.resource {
                    Resource::Texture(_) => Some(1),
                    Resource::TextureArray(v) => Some(v.len() as u32),
                    _ => None,
                };
                let ok = match (&res.resource, desc.kind) {
                    (_, BindingKind::SampledImage | BindingKind::StorageImage) => {
                        match texture_len {
                            Some(len) if desc.variable_count => {
                                let limit = self.max_variable_sampled_image_count();
                                if len > limit {
                                    bail!(
                                        "set {set_index} binding {}: {len} textures exceed \
                                         the device limit of {limit}",
                                        desc.binding
                                    );
                                }
                                variable_count = len;
                                true
                            }
                            Some(len) => len == desc.count,
                            None => false,
                        }
                    }
                    (Resource::Buffer(_), BindingKind::StorageBuffer | BindingKind::UniformBuffer) => {
                        true
                    }
                    (Resource::Sampler(_), BindingKind::Sampler) => true,
                    _ => false,
                };
                if !ok {
                    bail!(
                        "set {set_index} binding {} does not match the layout ({:?}, count {})",
                        desc.binding,
                        desc.kind,
                        desc.count
                    );
                }
            }
            variable_counts.push(variable_count);
        }
        Ok(variable_counts)
    }

    /// 検証済みのリソースをディスクリプタセットとして確保・書き込みする。
    pub(super) fn allocate_and_write_sets<'a>(
        &self,
        pool: vk::DescriptorPool,
        set_layouts: &[Arc<DescriptorSetLayout>],
        layout_desc: &PipelineLayoutDesc,
        sets: &'a [Vec<ResourceBinding<'a>>],
        variable_counts: &[u32],
        touched: &mut TouchedTextures<'a>,
    ) -> Result<Vec<vk::DescriptorSet>> {
        let mut vk_sets = Vec::with_capacity(sets.len());
        for ((layout, set_desc), &variable_count) in
            set_layouts.iter().zip(&layout_desc.sets).zip(variable_counts)
        {
            let variable = set_desc
                .bindings
                .iter()
                .any(|b| b.variable_count)
                .then_some(variable_count);
            vk_sets.push(self.allocate_descriptor_set(pool, layout.handle(), variable)?);
        }

        for ((set, set_desc), resources) in vk_sets.iter().zip(&layout_desc.sets).zip(sets) {
            for desc in &set_desc.bindings {
                let res = resources
                    .iter()
                    .find(|r| r.binding == desc.binding)
                    .expect("validated above");
                match &res.resource {
                    Resource::Buffer(buffer) => self.write_buffer_descriptor(
                        *set,
                        desc.binding,
                        if desc.kind == BindingKind::UniformBuffer {
                            vk::DescriptorType::UNIFORM_BUFFER
                        } else {
                            vk::DescriptorType::STORAGE_BUFFER
                        },
                        buffer.handle(),
                        0,
                        vk::WHOLE_SIZE,
                    ),
                    Resource::Sampler(sampler) => self.write_image_descriptor(
                        *set,
                        desc.binding,
                        0,
                        vk::DescriptorType::SAMPLER,
                        vk::ImageView::null(),
                        vk::ImageLayout::UNDEFINED,
                        sampler.handle(),
                    ),
                    Resource::Texture(_) | Resource::TextureArray(_) => {
                        let single;
                        let views: &[&TextureView] = match &res.resource {
                            Resource::Texture(v) => {
                                single = [*v];
                                &single
                            }
                            Resource::TextureArray(v) => v,
                            _ => unreachable!(),
                        };
                        let storage = desc.kind == BindingKind::StorageImage;
                        let descriptor_type = if storage {
                            vk::DescriptorType::STORAGE_IMAGE
                        } else {
                            vk::DescriptorType::SAMPLED_IMAGE
                        };
                        for (i, &view) in views.iter().enumerate() {
                            self.write_image_descriptor(
                                *set,
                                desc.binding,
                                i as u32,
                                descriptor_type,
                                view.handle(),
                                vk::ImageLayout::GENERAL,
                                vk::Sampler::null(),
                            );
                            if storage {
                                touched.storage.push(view.texture());
                            } else {
                                touched.sampled.push(view.texture());
                            }
                        }
                    }
                }
            }
        }
        Ok(vk_sets)
    }

    /// 束縛したテクスチャをGENERALレイアウトへ遷移させる(全テクスチャはGENERALで運用する)。
    pub(super) fn transition_touched(
        &self,
        cb: vk::CommandBuffer,
        touched: &TouchedTextures<'_>,
        shader_stages: vk::PipelineStageFlags,
    ) {
        for texture in &touched.sampled {
            texture.transition(
                self,
                cb,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                shader_stages,
                vk::AccessFlags::empty(),
                vk::AccessFlags::SHADER_READ,
            );
        }
        for texture in &touched.storage {
            texture.transition(
                self,
                cb,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                shader_stages,
                vk::AccessFlags::empty(),
                vk::AccessFlags::SHADER_WRITE,
            );
        }
    }
}
