use std::path::PathBuf;
use std::str::FromStr;
use anyhow::Result;
use crate::Dirs;

pub fn get_data_dir(dirs: &Dirs) -> Result<PathBuf> {
    let appdata_dir = PathBuf::from_str(&dirs.data_dir)?;
    if !appdata_dir.exists() {
        println!("Creating app data directory at {:?}", &appdata_dir);
        std::fs::create_dir_all(&appdata_dir)?;
    }
    Ok(appdata_dir)
}

pub fn get_local_data_dir(dirs: &Dirs) -> Result<PathBuf> {
    let local_data_dir = PathBuf::from_str(&dirs.local_data_dir)?;
    if !local_data_dir.exists() {
        println!("Creating local data directory at {:?}", &local_data_dir);
        std::fs::create_dir_all(&local_data_dir)?;
    }
    Ok(local_data_dir)
}
