// CPU 用 Lua の書き換え: 置き換えた箇所以外は元のテキストのまま、行番号が保たれること、
// 生成コードが LuaJIT の構文として正しいこと。

mod support;

use lua_gpu::codegen::layout::decode_header;
use lua_gpu::convert;
use support::*;

fn convert_ok(reg: &lua_gpu::spec::Registry, src: &str) -> lua_gpu::Converted {
    let out = convert(src, "main.lua", None, reg);
    let errors: Vec<String> = out
        .diagnostics
        .iter()
        .filter(|d| d.severity == lua_gpu::diag::Severity::Error)
        .map(|d| d.to_string())
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    out.result.expect("conversion")
}

/// 生成された Lua から 1 行目のプレリュードを除いたメインのソース(行の比較用)。
fn main_part(lua: &str) -> Vec<&str> {
    lua.lines().collect()
}

#[test]
fn code_without_kernels_is_kept_verbatim() {
    let src = "-- comment\nlocal a = 1   -- trailing\n\nprint(a)\n";
    let c = convert_ok(&obj_registry(), src);
    assert!(c.shaders.is_empty());
    let lines = main_part(&c.lua);
    assert_eq!(lines.len(), src.lines().count());
    // 1 行目はプレリュードの後ろに元の 1 行目が続く。
    assert!(lines[0].ends_with("-- comment"), "{}", lines[0]);
    assert_eq!(&lines[1..], &src.lines().collect::<Vec<_>>()[1..]);
    assert!(decode_header(&c.lua).unwrap().kernels.is_empty());
}

#[test]
fn kernels_are_replaced_and_line_numbers_are_kept() {
    let src = r#"local w, h = obj.w, obj.h
for y = 0, h - 1 do
  for x = 0, w - 1 do
    local r, g, b, a = obj.getpixel(x, y)
    obj.putpixel(x, y, 255 - r, g, b, a)
  end
end
print("after")
"#;
    let c = convert_ok(&obj_registry(), src);
    assert_eq!(c.shaders.len(), 1);
    let lines = main_part(&c.lua);
    assert_eq!(lines.len(), src.lines().count());
    // CpuFunction(property)の読み取りは生成関数の呼び出しになる。
    assert!(lines[0].contains("local w, h = __lg_cf[0](), __lg_cf[1]()"), "{}", lines[0]);
    assert!(lines[1].starts_with("do local __lg_s0"), "{}", lines[1]);
    assert!(lines[1].contains("__lg_dispatch(0, {"), "{}", lines[1]);
    assert_eq!(lines[7], "print(\"after\")");
    // レイアウト: 総数 + 2 次元ぶん(開始・刻み・回数)。
    assert_eq!(decode_header(&c.lua).unwrap().kernels[0].fields.len(), 7);
    lua_gpu::frontend::parse(&c.lua).expect("generated Lua must parse");
}

#[test]
fn replay_and_records_are_generated() {
    let src = r#"local count = 0
for x = 0, 9 do
  count = count + 1
  obj.putpixel(x, 0, count, math.random() * 255, 0, 255)
end
print(count)
"#;
    let c = convert_ok(&obj_registry(), src);
    support::dump::dump("replay.lua", &c.lua);
    let line = c.lua.lines().nth(1).unwrap();
    // 再生のループで count を更新し、count と乱数を記録する。
    assert!(line.contains("for x = __lg_s0, __lg_e0, __lg_t0 do"), "{line}");
    assert!(line.contains("count = (count + 1)"), "{line}");
    assert!(line.contains("__lg_r(__lg_rec, __lg_b + 0, count)"), "{line}");
    assert!(line.contains("__lg_r(__lg_rec, __lg_b + 1, math.random())"), "{line}");
    lua_gpu::frontend::parse(&c.lua).expect("generated Lua must parse");
}

#[test]
fn gpu_calls_outside_kernels_in_cpu_code_raise_at_runtime() {
    // CPU からも呼ばれ得る関数の中の GPU 関数呼び出し(カーネル内でだけ使われる関数は GPU 側に変換)。
    let src = r#"local function plot(x) obj.putpixel(x, 0, 1, 1, 1, 1) end
for x = 0, 3 do plot(x) end
"#;
    let c = convert_ok(&obj_registry(), src);
    // plot の定義はそのまま残るが、CPU で呼ばれたら実行時エラーにする。
    assert!(c.lua.contains("__lg_gpu_only(\"obj.putpixel\")"), "{}", c.lua);
    lua_gpu::frontend::parse(&c.lua).expect("generated Lua must parse");
}
