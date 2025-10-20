use std::{fs::File, io::Write};

use anyhow::{ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dir_util::get_data_dir;
use crate::Dirs;
use crate::error::AperioResult;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PythonConfig {
    pub default_version: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub python: PythonConfig,
}

pub fn init_config(dirs: &Dirs) -> AperioResult<()> {
    // 設定ファイルがあるか確認し、なければdefault-config.jsonをコピー
    let appdata_dir = get_data_dir(dirs)?;
    let config_path = appdata_dir.join("config.json");
    if !config_path.exists() {
        let config_bytes = include_bytes!("data/default-config.json");
        let mut file = File::create(&config_path)?;
        file.write(config_bytes)?;
        file.sync_data()?;
        println!("Default config copied to {:?}", config_path);
    } else {
        println!("Config file found at {:?}", config_path);
    }

    Ok(())
}

pub fn read_config(dirs: &Dirs) -> AperioResult<AppConfig> {
    let appdata_dir = get_data_dir(dirs)?;
    let config_path = appdata_dir.join("config.json");
    ensure!(config_path.exists());

    let config = std::fs::read_to_string(&config_path)?;
    let config: AppConfig = match serde_json::from_str(&config) {
        Ok(config) => config,
        Err(err) => {
            println!(
                "Failed to parse config.json: {}. Trying to merge with default config.",
                err
            );
            let merged_config = merge_configs(&config)?;
            let config: AppConfig = serde_json::from_value(merged_config)
                .expect("Failed to parse config even after merging with default config.");

            // 成功したなら保存
            let config_str = serde_json::to_string_pretty(&config)?;
            std::fs::write(&config_path, config_str)?;

            config
        }
    };
    Ok(config)
}

fn merge_configs(config: &str) -> AperioResult<Value> {
    let default_config_bytes = include_bytes!("data/default-config.json");
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
