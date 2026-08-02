mod ffi {
    #![allow(non_upper_case_globals, non_camel_case_types, dead_code, clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/avloader_bindings.rs"));
}

mod audio_loader;
mod common;
mod frame_cache;
mod image_loader;
mod video_loader;
mod yuv_pipeline;

pub use audio_loader::AudioLoader;
pub use common::ColorFormat;
pub use image_loader::ImageLoader;
pub use video_loader::VideoLoader;
