// デバッグ用: LUA_GPU_DUMP=<ディレクトリ> のとき、生成物をファイルに書き出す。
#![allow(dead_code)]

pub fn dump(name: &str, content: &str) {
    if let Some(dir) = std::env::var_os("LUA_GPU_DUMP") {
        let _ = std::fs::write(std::path::Path::new(&dir).join(name), content);
    }
}
