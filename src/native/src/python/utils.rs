use crate::util::get_local_data_dir;
use crate::Dirs;
use anyhow::{bail, Context, Result};
use pyo3::ffi::*;
use pyo3::prelude::PyAnyMethods;
use pyo3::Python;
use std::env::consts;
use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::{env, fs};
use toml_edit::DocumentMut;

pub struct PythonStatus {
    pub installed: bool,
    pub version: Option<String>,
}

pub fn get_base_args(appdata_dir: &str) -> Vec<&str> {
    vec!["--directory", appdata_dir, "--no-cache"]
}

fn file_extension(base_dir: &Path, s: &str) -> PathBuf {
    // windowsならexeをいれて返却
    if cfg!(target_os = "windows") {
        // PathBufにして合体、文字列にして返却
        return base_dir.join(format!("{}.exe", s));
    }

    // PathBufにして合体、文字列にして返却
    base_dir.join(s)
}

fn run_uv(dir: &Dirs, args: Vec<&str>) -> Result<String> {
    let bin_dir = PathBuf::from_str(&dir.resource_dir)?.join("bin");
    let output = Command::new(file_extension(&bin_dir, "uv"))
        .args(args)
        .output()?;

    if !output.status.success() {
        bail!(String::from_utf8(output.stderr.clone())?);
    }

    Ok(String::from_utf8(output.stdout.clone())?)
}

pub fn add_python_path_env(dir: &Dirs) -> Result<()> {
    // PYTHONPATHとPYTHONHOMEの設定
    let local_data_dir = get_local_data_dir(dir)?; // 環境ファイルがある
    let python_path = local_data_dir.join("python"); // pythonがある
                                                     // linux/macならbin/python、windowsならpython.exe
    let bin_path = if cfg!(target_os = "windows") {
        file_extension(&python_path, "python") // pythonの実行ファイルがある
    } else {
        file_extension(&python_path.join("bin"), "python") // pythonの実行ファイルがある
    };

    unsafe {
        env::set_var("UV_PROJECT_ENVIRONMENT", &python_path);

        let python_path = CString::new(
            python_path
                .to_str()
                .context("Failed to convert python path to str")?,
        )?;
        let bin_path = CString::new(
            bin_path
                .to_str()
                .context("Failed to convert python bin path to str")?,
        )?;

        let mut config: PyPreConfig = std::mem::zeroed();
        PyPreConfig_InitIsolatedConfig(&mut config);
        config.utf8_mode = 1;
        let err = Py_PreInitialize(&mut config);
        if PyStatus_Exception(err) != 0 {
            bail!(
                "Failed to pre-initialize embedded Python interpreter\nmsg: {:?}",
                CStr::from_ptr(err.err_msg)
            );
        }

        let mut config: PyConfig = std::mem::zeroed();
        PyConfig_InitIsolatedConfig(&mut config);
        PyConfig_SetBytesString(&mut config, &mut config.home, python_path.as_ptr());
        PyConfig_SetBytesString(&mut config, &mut config.executable, bin_path.as_ptr());

        let err = Py_InitializeFromConfig(&mut config);
        PyConfig_Clear(&mut config);
        if PyStatus_Exception(err) != 0 {
            bail!(
                "Failed to initialize embedded Python interpreter\nmsg: {:?}",
                CStr::from_ptr(err.err_msg)
            );
        }
    }

    Ok(())
}
pub fn check_python_installed(dir: &Dirs) -> Result<PythonStatus> {
    // appdataのdir pathを取得
    let appdata_dir = get_local_data_dir(dir)?;
    // python/bin/python(.exe)のpathを取得
    let python_path = if cfg!(target_os = "windows") {
        file_extension(&appdata_dir.join("python"), "python")
    } else {
        file_extension(&appdata_dir.join("python").join("bin"), "python")
    };
    println!("Checking for Python at path: {:?}", python_path);

    // pythonが存在するか確認
    if !python_path.exists() {
        println!("Python executable not found at {:?}", python_path);
        return Ok(PythonStatus {
            installed: false,
            version: None,
        });
    }

    // pythonのversionを取得
    // libpythonとvenv(uvが作った環境)のバージョンが全て合わなければCライブラリ系が読み込めないっぽい?
    // https://github.com/axnsan12/drf-yasg/issues/362#issuecomment-515542184
    let python_version = Python::attach(|py| -> Result<String> {
        let sys = py.import("sys")?;
        let version = sys.getattr("version_info")?;
        let major: i32 = version.get_item(0)?.extract()?;
        let minor: i32 = version.get_item(1)?.extract()?;
        let micro: i32 = version.get_item(2)?.extract()?;
        println!("Embed Python version: {}", sys.getattr("version")?);

        Ok(format!("{}.{}.{}", major, minor, micro))
    })?;

    // インストールされているpythonのversionを取得
    let installed_python_version = String::from_utf8(
        Command::new(&python_path)
            .args([
                "-c",
                "import sys; v=sys.version_info; print(f'{v[0]}.{v[1]}.{v[2]}')",
            ])
            .output()?
            .stdout,
    )?;
    let installed_python_version = installed_python_version.trim(); // 改行を削除

    if installed_python_version != python_version {
        println!(
            "Python version mismatch: expected(embed libpython) {}, found(installed python) {}. Try reinstalling.",
            python_version, installed_python_version
        );
        // ディレクトリを削除
        fs::remove_dir_all(appdata_dir.join("python")).ok();
        return Ok(PythonStatus {
            installed: false,
            version: Some(python_version),
        });
    }

    Ok(PythonStatus {
        installed: true,
        version: Some(python_version),
    })
}

pub fn install_packages(dir: &Dirs, packages: Vec<&str>) -> Result<()> {
    // appdataのdir pathを取得
    let appdata_path = get_local_data_dir(dir)?;
    let appdata_dir = appdata_path
        .to_str()
        .context("Failed to convert appdata path to str")?;

    let python_dir = appdata_path.join("python");
    let mut args = vec!["add"];
    args.extend(packages);
    args.extend([
        "--no-python-downloads",
        "--python",
        python_dir
            .to_str()
            .context("Failed to convert python path to str")?,
        "--no-sync",
    ]);
    args.extend(get_base_args(appdata_dir));

    run_uv(dir, args)?;
    Ok(())
}

pub fn install_python(dir: &Dirs, python_version: &str, is_vague: bool) -> Result<()> {
    // appdataのdir pathを取得
    let appdata_path = get_local_data_dir(dir)?;
    let appdata_dir = appdata_path
        .to_str()
        .context("Failed to convert appdata path to str")?;

    // uv initをする
    // pyproject.tomlがあれば、手動で変更する
    let python_version_str = if is_vague {
        // vagueなら~=をつける
        format!("~={}", python_version)
    } else {
        format!("=={}", python_version)
    };

    let appdata_toml = appdata_path.join("pyproject.toml");
    if appdata_toml.exists() {
        let pj_data = fs::read_to_string(&appdata_toml)?;
        let mut pj_data = pj_data.parse::<DocumentMut>()?;
        pj_data["project"]["requires-python"] = toml_edit::value(&python_version_str);
        fs::write(&appdata_toml, pj_data.to_string())?;
        println!(
            "Updated pyproject.toml with requires-python = {}",
            &python_version_str
        );
    } else {
        let mut args = vec![
            "init",
            "--python",
            &python_version_str,
            "--bare",
            "--author-from",
            "none",
            "--name",
            "aperio-env",
        ];
        args.extend(get_base_args(appdata_dir));
        run_uv(dir, args)?;
    }

    // appdata_dirにpythonディレクトリかcpythonから始まるディレクトリ(複数あるかも)があれば削除
    if appdata_path.join("python").exists() {
        fs::remove_dir_all(appdata_path.join("python"))?;
    }
    if let Ok(entries) = fs::read_dir(&appdata_path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();
            if file_name_str.starts_with("cpython") {
                fs::remove_dir_all(entry.path()).ok();
            }
        }
    }

    // uv python installコマンドを実行してpythonをインストール
    let python_type = format!("cpython-{}-{}-{}", python_version, consts::OS, consts::ARCH);
    let mut args = vec![
        "python",
        "install",
        "--no-bin",
        "--install-dir",
        appdata_dir,
        "--project",
        appdata_dir,
        python_type.as_str(),
    ];
    args.extend(get_base_args(appdata_dir));
    run_uv(dir, args)?;

    // 何故かゴミができるのであれば削除
    fs::remove_file(appdata_path.join(".gitignore")).ok();
    fs::remove_file(appdata_path.join(".lock")).ok();
    fs::remove_dir(appdata_path.join(".temp")).ok();

    // cpythonから始まるディレクトリができるので、pythonにリネーム
    let cpython_dir = fs::read_dir(&appdata_path)?
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            let binding = entry.file_name();
            let file_name = binding.to_string_lossy();
            file_name.starts_with("cpython")
        })
        .map(|entry| entry.path())
        .context("No cpython directory found in app data dir")?;

    fs::rename(cpython_dir, appdata_path.join("python")).ok();

    // uv addコマンドを実行してnumpyをインストール
    install_packages(dir, vec!["numpy"])?;
    println!("Successfully installed Python and required packages");

    Ok(())
}

pub fn sync_packages(dir: &Dirs) -> Result<String> {
    let appdata_dir = get_local_data_dir(dir)?;
    let python_dir = appdata_dir.join("python");
    let lock_file = appdata_dir.join("pylock.toml");
    let appdata_dir = appdata_dir
        .to_str()
        .context("Failed to convert appdata path to str")?;
    let python_dir = python_dir
        .to_str()
        .context("Failed to convert python path to str")?;
    let lock_file = lock_file
        .to_str()
        .context("Failed to convert lock file path to str")?;

    // pylock.tomlを生成
    let mut args = vec![
        "export",
        "--python",
        python_dir,
        "--format",
        "pylock.toml",
        "--no-header",
        "-o",
        lock_file,
    ];
    args.extend(get_base_args(appdata_dir));

    run_uv(dir, args)?;
    println!("Sync packages success");

    // pylock.tomlをもとにuv pip syncでパッケージを同期
    let mut args = vec![
        "pip",
        "sync",
        "--no-python-downloads",
        "--python",
        python_dir,
        "--break-system-packages",
    ];
    args.extend(get_base_args(appdata_dir));
    args.extend([lock_file]);

    Ok(run_uv(dir, args)?)
}
