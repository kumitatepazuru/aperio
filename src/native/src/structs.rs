use napi_derive::napi;

#[napi(object)]
pub struct Dirs {
    pub data_dir: String,
    pub local_data_dir: String,
    pub resource_dir: String,
    pub plugin_manager_dir: String,
    pub default_plugins_dir: String,
    pub dist_dir: String,
}
