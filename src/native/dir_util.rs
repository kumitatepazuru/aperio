use std::path::PathBuf;
use std::str::FromStr;
use crate::Dirs;
use crate::error::AperioResult;

pub fn get_data_dir(dirs: &Dirs) -> AperioResult<PathBuf> {
    let appdata_dir = PathBuf::from_str(&dirs.data_dir)?;
    if !appdata_dir.exists() {
        println!("Creating app data directory at {:?}", &appdata_dir);
        std::fs::create_dir_all(&appdata_dir)?;
    }
    Ok(appdata_dir)
}

pub fn get_local_data_dir(dirs: &Dirs) -> AperioResult<PathBuf> {
    let local_data_dir = PathBuf::from_str(&dirs.local_data_dir)?;
    if !local_data_dir.exists() {
        println!("Creating local data directory at {:?}", &local_data_dir);
        std::fs::create_dir_all(&local_data_dir)?;
    }
    Ok(local_data_dir)
}
