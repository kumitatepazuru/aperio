pub mod common_pipeline;
pub mod compiled_func;
pub mod compiled_wgsl;
pub mod image_generate_builder;
pub mod image_generator;
pub mod image_pixel_format;
pub mod resource_pool;
pub mod texture_to_native;
pub mod compose_wgsl;

pub use image_pixel_format::ImagePixelFormat;

/// GPU テクスチャの共有フォーマット。
#[derive(Debug, Clone, PartialEq)]
pub enum SharedTextureFormat {
    Rgba16Float,
    Bgra8Unorm,
}
