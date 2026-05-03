mod ffi {
    #![allow(non_upper_case_globals, non_camel_case_types, dead_code, clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/avloader_bindings.rs"));
}

mod frame_cache;
mod video_loader;
mod yuv_pipeline;

pub use video_loader::{ColorFormat, VideoLoader};
