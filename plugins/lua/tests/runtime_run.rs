// 実行時: CPU 用 Lua の実行、params のバイト列、記録、ホスト関数、外部値の検査、エラー、モジュール。

mod support;

use std::fs;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use lua_gpu::runtime::{ExtValue, RunError, Runtime};
use lua_gpu::spec::{FunctionSpec, PyFunction, Registry};
use lua_gpu::{convert, Converted};
use support::*;

fn convert_ok(reg: &Registry, src: &str, base: Option<&std::path::Path>) -> Converted {
    let out = convert(src, "main.lua", base, reg);
    let errors: Vec<String> = out
        .diagnostics
        .iter()
        .filter(|d| d.severity == lua_gpu::diag::Severity::Error)
        .map(|d| d.to_string())
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    out.result.expect("conversion")
}

fn ext(pairs: &[(&str, f64)]) -> IndexMap<String, ExtValue> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), ExtValue::Number(*v)))
        .collect()
}

fn run(rt: &Runtime, lua: &str, ext: &IndexMap<String, ExtValue>) -> Result<Vec<(u32, Vec<u32>)>, RunError> {
    let mut out = Vec::new();
    rt.run(lua, ext, &mut |k, bytes| {
        let words = bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        out.push((k, words));
        Ok(())
    })?;
    Ok(out)
}

const INVERT: &str = r#"
local w, h = obj.w, obj.h
for y = 0, h - 1 do
  for x = 0, w - 1 do
    local r, g, b, a = obj.getpixel(x, y)
    obj.putpixel(x, y, 255 - r, 255 - g, 255 - b, a)
  end
end
"#;

#[test]
fn dispatch_params_follow_the_layout() {
    let reg = obj_registry();
    let c = convert_ok(&reg, INVERT, None);
    let rt = Runtime::new(&reg).unwrap();
    let d = run(&rt, &c.lua, &ext(&[("w", 4.0), ("h", 3.0)])).unwrap();
    assert_eq!(d.len(), 1);
    let (k, w) = &d[0];
    assert_eq!(*k, 0);
    let f = |i: usize| f32::from_bits(w[i]);
    // 総数, (y: 開始, 刻み, 回数), (x: 開始, 刻み, 回数)
    assert_eq!(w[0], 12);
    assert_eq!((f(1), f(2), w[3]), (0.0, 1.0, 3));
    assert_eq!((f(4), f(5), w[6]), (0.0, 1.0, 4));
    // 2 回目の実行(キャッシュ済みのチャンク)でも同じ結果。
    assert_eq!(run(&rt, &c.lua, &ext(&[("w", 4.0), ("h", 3.0)])).unwrap(), d);
    // 空のループでもディスパッチはする(総数 0)。
    let d0 = run(&rt, &c.lua, &ext(&[("w", 0.0), ("h", 3.0)])).unwrap();
    assert_eq!(d0[0].1[0], 0);
}

#[test]
fn replay_records_values_per_iteration() {
    let reg = obj_registry();
    let src = r#"
local count = 10
for x = 0, 4 do
  count = count + 1
  obj.putpixel(x, 0, count, 0, 0, 255)
end
assert(count == 15)
"#;
    let c = convert_ok(&reg, src, None);
    let rt = Runtime::new(&reg).unwrap();
    let d = run(&rt, &c.lua, &ext(&[("w", 1.0), ("h", 1.0)])).unwrap();
    let w = &d[0].1;
    // 総数, 開始, 刻み, 回数, 記録ブロックの位置 → その位置から 5 個の記録。
    assert_eq!(w[0], 5);
    let base = w[4] as usize;
    let recs: Vec<f32> = (0..5).map(|i| f32::from_bits(w[base + i])).collect();
    assert_eq!(recs, vec![11.0, 12.0, 13.0, 14.0, 15.0]);
}

#[test]
fn captured_tables_are_packed_into_the_tail() {
    let reg = obj_registry();
    let src = r#"
local weights = {1, 2, 3}
local conf = { gain = 0.5, offs = {-1, 1} }
for x = 0, 1 do
  obj.putpixel(x, 0, weights[x + 1] * conf.gain + conf.offs[x + 1], 0, 0, 255)
end
"#;
    let c = convert_ok(&reg, src, None);
    let rt = Runtime::new(&reg).unwrap();
    let d = run(&rt, &c.lua, &ext(&[("w", 1.0), ("h", 1.0)])).unwrap();
    let w = &d[0].1;
    let f = |i: usize| f32::from_bits(w[i]);
    // 固定領域: 総数, 開始, 刻み, 回数, weights(off, len), conf(gain, offs(off, len))
    let (woff, wlen) = (w[4] as usize, w[5] as usize);
    assert_eq!(wlen, 3);
    assert_eq!((f(woff), f(woff + 1), f(woff + 2)), (1.0, 2.0, 3.0));
    assert_eq!(f(6), 0.5);
    let (ooff, olen) = (w[7] as usize, w[8] as usize);
    assert_eq!(olen, 2);
    assert_eq!((f(ooff), f(ooff + 1)), (-1.0, 1.0));
}

#[test]
fn host_functions_and_runtime_errors() {
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let l2 = log.clone();
    let mut functions = vec![FunctionSpec::Py(PyFunction {
        name: "host.log".into(),
        func: Arc::new(move |_, args: mlua::MultiValue| {
            let s: Vec<String> = args.iter().map(|v| format!("{v:?}")).collect();
            l2.lock().unwrap().push(s.join(","));
            Ok(mlua::MultiValue::new())
        }),
        returns: None,
    })];
    functions.extend(obj_registry().functions);
    let reg = Registry::new(functions, Default::default()).unwrap();
    let src = "host.log('start')\nfor x = 0, 2 do host.log(x) obj.putpixel(x, 0, 1, 1, 1, 1) end\nhost.log('end')";
    let c = convert_ok(&reg, src, None);
    let rt = Runtime::new(&reg).unwrap();
    run(&rt, &c.lua, &ext(&[("w", 1.0), ("h", 1.0)])).unwrap();
    // ループの中のホスト関数は、元の回数・順序のまま CPU の再生で呼ばれる。
    assert_eq!(log.lock().unwrap().len(), 5);

    // Lua の実行時エラーはメッセージ付きで返る。
    let c = convert_ok(&reg, "local t = nil\nprint(t.x)", None);
    match run(&rt, &c.lua, &ext(&[("w", 1.0), ("h", 1.0)])) {
        Err(RunError::Lua(m)) => assert!(m.contains("attempt to index"), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn externals_are_validated() {
    let reg = obj_registry();
    let c = convert_ok(&reg, INVERT, None);
    let rt = Runtime::new(&reg).unwrap();
    assert!(matches!(run(&rt, &c.lua, &ext(&[("w", 1.0)])), Err(RunError::Arg(_))));
    assert!(matches!(run(&rt, &c.lua, &ext(&[("w", 1.5), ("h", 1.0)])), Err(RunError::Arg(_))));
    let mut bad = ext(&[("w", 1.0), ("h", 1.0)]);
    bad.insert("zz".into(), ExtValue::Bool(true));
    assert!(matches!(run(&rt, &c.lua, &bad), Err(RunError::Arg(_))));
}

#[test]
fn each_run_starts_from_a_fresh_environment() {
    let reg = obj_registry();
    let c = convert_ok(&reg, "assert(counter == nil)\ncounter = 1", None);
    let rt = Runtime::new(&reg).unwrap();
    run(&rt, &c.lua, &ext(&[("w", 1.0), ("h", 1.0)])).unwrap();
    run(&rt, &c.lua, &ext(&[("w", 1.0), ("h", 1.0)])).unwrap();
}

#[test]
fn bundled_modules_and_dofile() {
    let dir = std::env::temp_dir().join(format!("lua_gpu_rt_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("util.lua"), "loads = (loads or 0) + 1\nlocal M = {}\nfunction M.twice(v) return v * 2 end\nreturn M").unwrap();
    fs::write(dir.join("conf.lua"), "return 21").unwrap();
    let reg = obj_registry();
    let src = r#"
local u = require("util")
local u2 = require("util")
assert(u == u2 and loads == 1)
local k = dofile("conf.lua")
for x = 0, 3 do
  obj.putpixel(x, 0, u.twice(x) + k, 0, 0, 255)
end
"#;
    let c = convert_ok(&reg, src, Some(&dir));
    // 同梱済みなので、実行時にファイルが無くても動く。
    fs::remove_dir_all(&dir).ok();
    let rt = Runtime::new(&reg).unwrap();
    for _ in 0..2 {
        let d = run(&rt, &c.lua, &ext(&[("w", 1.0), ("h", 1.0)])).unwrap();
        assert_eq!(d.len(), 1);
    }
    // u.twice は GPU 側の関数として変換され、k は捕捉される。
    assert!(c.shaders[0].contains("M.twice"), "{}", c.shaders[0]);
}
