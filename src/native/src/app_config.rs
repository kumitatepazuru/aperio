use napi_derive::napi;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::{fs::File, io::Write};

use crate::node_shared_texture::NodeSharedTextureFormat;
use crate::util::get_data_dir;
use crate::Dirs;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[napi(object)]
pub struct PythonConfig {
    pub default_version: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[napi(object)]
pub struct AperioConfig {
    pub python: PythonConfig,
    pub fast_preview: bool,
    pub tex_pixel_format: NodeSharedTextureFormat,
    pub dock_layout: Option<Value>,
}

const DEFAULT_CONFIG: &str = include_str!("data/default-config.json");

#[derive(Clone)]
#[napi]
pub struct AperioConfigManager {
    config_path: PathBuf,
    config: AperioConfig,
}

#[napi]
impl AperioConfigManager {
    #[napi(constructor)]
    pub fn new(dirs: Dirs) -> napi::Result<Self> {
        // 設定ファイルがあるか確認し、なければdefault-config.jsonをコピー
        let appdata_dir =
            get_data_dir(&dirs).map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let config_path = appdata_dir.join("config.json");
        let config_str = if !config_path.exists() {
            let config_bytes = DEFAULT_CONFIG.as_bytes();
            let mut file = File::create(&config_path)?;
            file.write(config_bytes)?;
            file.sync_data()?;
            println!("Default config copied to {:?}", config_path);

            String::from(DEFAULT_CONFIG)
        } else {
            println!("Config file found at {:?}", config_path);

            fs::read_to_string(&config_path)?
        };

        let config: AperioConfig = match serde_json::from_str(&config_str) {
            Ok(config) => config,
            Err(err) => {
                println!(
                    "Failed to parse config.json: {}. Trying to merge with default config.",
                    err
                );
                let merged_config = merge_configs(&config_str)?;
                let config: AperioConfig = serde_json::from_value(merged_config)?;

                // 成功したなら保存
                let config_str = serde_json::to_string_pretty(&config)?;
                fs::write(&config_path, config_str)?;

                config
            }
        };

        Ok(AperioConfigManager {
            config_path,
            config,
        })
    }

    #[napi(getter, js_name = "config")]
    pub fn get_config(&self) -> AperioConfig {
        self.config.clone()
    }

    #[napi]
    pub fn set_config(&mut self, config: AperioConfig) -> napi::Result<()> {
        let config_str = serde_json::to_string_pretty(&config)?;
        self.config = config;
        fs::write(&self.config_path, config_str)?;
        println!("Config written to {:?}", self.config_path);
        Ok(())
    }
}

fn merge_configs(config: &str) -> napi::Result<Value> {
    let default_config_bytes = DEFAULT_CONFIG.as_bytes();
    let default_config: Value = serde_json::from_slice(default_config_bytes)?;
    let user_config: Value = serde_json::from_str(config)?;
    let mut merged_config = default_config;
    merge(&mut merged_config, user_config);

    Ok(merged_config)
}

fn merge(a: &mut Value, b: Value) {
    if let Value::Object(a) = a {
        if let Value::Object(b) = b {
            for (k, v) in b {
                if v.is_null() {
                    a.remove(&k);
                } else {
                    merge(a.entry(k).or_insert(Value::Null), v);
                }
            }

            return;
        }
    }

    *a = b;
}
