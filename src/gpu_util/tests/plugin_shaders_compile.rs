// `plugins/`配下のすべてのエフェクトシェーダーが実際にコンパイルできること、
// および`gpu_util`側が決め打ちする配置(set 0 = inputs, set 1 = res)と食い違わないよう、
// `ParameterBlock<InputTextures*> inputs;`/`ParameterBlock<EffectResources*> res;`
// の直前に`[vk::binding(0, 0)]`/`[vk::binding(0, 1)]`が付与されていることを
// 一括で確認する回帰テスト。

use gpu_util::compiled_slang::compile_slang_to_spirv;
use std::path::{Path, PathBuf};

const APERIO_LIB_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugins/base/common/lib");
const PLUGINS_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugins");

fn collect_slang_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_slang_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("slang") {
            out.push(path);
        }
    }
}

#[test]
fn verify_all_effect_shaders_compile_with_pinned_bindings() {
    let mut files = Vec::new();
    collect_slang_files(Path::new(PLUGINS_ROOT), &mut files);
    files.sort();

    let mut failures = Vec::new();
    let mut checked = 0usize;

    for path in &files {
        let source = std::fs::read_to_string(path).expect("failed to read shader source");
        // res/inputsを両方持つ「実際のエフェクトシェーダー」だけを対象にする
        // (common/lib配下のimport専用モジュールや、common.slangのような
        // ヘルパー限定ファイルは対象外)。
        if !source.contains("ParameterBlock<") || !source.contains("res;") {
            continue;
        }

        checked += 1;

        // 静的チェック: gpu_util側の決め打ち(set 0 = inputs, set 1 = res)と
        // 食い違わないよう、宣言直前にpinが付いていること。
        if source.contains("inputs;") && !source.contains("[vk::binding(0, 0)]\nParameterBlock") {
            failures.push(format!(
                "{}: 'inputs' declaration is missing the required [vk::binding(0, 0)] pin",
                path.display()
            ));
        }
        if !source.contains("[vk::binding(0, 1)]\nParameterBlock") {
            failures.push(format!(
                "{}: 'res' declaration is missing the required [vk::binding(0, 1)] pin",
                path.display()
            ));
        }

        let dir = path.parent().unwrap().to_string_lossy().to_string();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();

        if let Err(e) = compile_slang_to_spirv(
            &name,
            &path.to_string_lossy(),
            &source,
            "main",
            &[&dir, APERIO_LIB_DIR],
            &[("APERIO_IMAGE_FORMAT", "\"rgba16f\"")],
        ) {
            failures.push(format!("{}: {e:#}", path.display()));
        }
    }

    println!("checked {checked} effect shader(s)");
    if !failures.is_empty() {
        panic!("{} shader(s) failed:\n{}", failures.len(), failures.join("\n\n"));
    }
}
