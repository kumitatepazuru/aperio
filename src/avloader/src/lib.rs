mod ffi {
    #![allow(non_upper_case_globals, non_camel_case_types, dead_code, clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/avloader_bindings.rs"));
}

mod yuv_pipeline;
mod video_loader;

pub use video_loader::{ColorFormat, VideoLoader};
