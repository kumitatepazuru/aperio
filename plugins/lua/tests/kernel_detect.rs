// カーネル検出: 並列次元の決め方、CPU に残るループ、単一スレッドのカーネル、GPU 専用の関数。

mod support;

use support::*;

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
fn perfect_nest_becomes_a_2d_kernel() {
    let a = analyze_ok(&obj_registry(), INVERT);
    assert_eq!(a.plan.kernels.len(), 1);
    let k = &a.plan.kernels[0];
    assert_eq!(k.dims.len(), 2);
    assert_eq!(a.program.var(k.dims[0].var).name, "y");
    assert_eq!(a.program.var(k.dims[1].var).name, "x");
    assert!(!k.needs_replay, "no CPU work inside the loop");
    assert!(k.records.is_empty() && k.hoists.is_empty());
    // w / h はループの上下限にしか使われない。上下限はディスパッチ前に CPU で評価して
    // 次元の params として渡すので、GPU コードからの捕捉は無い。
    assert!(k.captures.is_empty(), "{:?}", k.captures);
}

#[test]
fn values_used_in_the_body_are_captured() {
    let src = r#"
local w = obj.w
for x = 0, 9 do obj.putpixel(x, 0, w - x, 0, 0, 255) end"#;
    let a = analyze_ok(&obj_registry(), src);
    assert_eq!(a.plan.kernels[0].captures.len(), 1);
}

#[test]
fn imperfect_nest_parallelizes_the_outer_loop_only() {
    let src = r#"
for y = 0, 9 do
  local row = y * 2
  for x = 0, 9 do obj.putpixel(x, row, 1, 1, 1, 1) end
end"#;
    let a = analyze_ok(&obj_registry(), src);
    assert_eq!(a.plan.kernels[0].dims.len(), 1);

    // 内側の上下限が外側のループ変数に依存する(三角形)場合も外側だけ。
    let src = "for y = 0, 9 do for x = 0, y do obj.putpixel(x, y, 1, 1, 1, 1) end end";
    let a = analyze_ok(&obj_registry(), src);
    assert_eq!(a.plan.kernels[0].dims.len(), 1);
}

#[test]
fn loops_that_cannot_be_parallelized_stay_on_the_cpu() {
    let src = r#"
for i = 1, 10 do
  if i > 5 then break end
  obj.putpixel(i, 0, 1, 1, 1, 1)
end"#;
    let (a, d) = analyze(&obj_registry(), src);
    assert!(!d.has_errors(), "{d}");
    let codes: Vec<_> = d.warnings().map(|w| w.code).collect();
    assert!(codes.contains(&"not-parallelized"), "{codes:?}");
    assert!(codes.contains(&"single-thread-kernel"), "{codes:?}");
    // ループは CPU に残り、putpixel の文だけが単一スレッドのカーネルになる。
    let k = &a.unwrap().plan.kernels[0];
    assert!(k.dims.is_empty() && k.single_thread);
}

#[test]
fn top_level_gpu_calls_are_grouped_with_their_users() {
    let src = r#"
local r, g, b, a = obj.getpixel(0, 0)
local m = (r + g + b) / 3
obj.putpixel(1, 1, m, m, m, a)
print("done")"#;
    let a = analyze_ok(&obj_registry(), src);
    assert_eq!(a.plan.kernels.len(), 1);
    // getpixel / 平均 / putpixel の 3 文が 1 つのカーネル。print は CPU。
    assert_eq!(a.plan.kernels[0].stmts.len(), 3);
}

#[test]
fn gpu_values_cannot_flow_back_to_the_cpu() {
    let codes = error_codes(&obj_registry(), "local r = obj.getpixel(0, 0)\nprint(r)");
    assert!(codes.contains(&"cpu-needs-gpu-value") || codes.contains(&"cpu-call-gpu-data"), "{codes:?}");
}

#[test]
fn helper_functions_called_only_from_kernels_are_translated() {
    let src = r#"
local function plot(x, y, v) obj.putpixel(x, y, v, v, v, 255) end
for y = 0, 3 do for x = 0, 3 do plot(x, y, x * 10) end end"#;
    let a = analyze_ok(&obj_registry(), src);
    // plot の中に別のカーネルは作られず、カーネル内で GPU 関数として使われる。
    assert_eq!(a.plan.kernels.len(), 1);
    let k = &a.plan.kernels[0];
    assert_eq!(k.funcs.len(), 1);
    assert_eq!(a.program.func(k.funcs[0]).name.as_deref(), Some("plot"));
    assert_eq!(k.func_types[&k.funcs[0]].params.len(), 3);
}
