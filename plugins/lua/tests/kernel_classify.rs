// カーネル本体の振り分け: 持ち出し(hoist)、再生での記録(record)、回数可変のループ、エラー。

mod support;

use lua_gpu::kernel::{CaptureSource, GpuTy};
use support::*;

#[test]
fn loop_invariant_file_reads_are_hoisted() {
    // 「外部ファイルを途中で読み込む」処理: ループ不変な読み取りなので、ディスパッチ前に 1 回だけ実行する。
    let src = r#"
for y = 0, 9 do
  for x = 0, 9 do
    local gain = tonumber(io.open("gain.txt"):read("*a"))
    local r = obj.getpixel(x, y)
    obj.putpixel(x, y, r * gain, 0, 0, 255)
  end
end"#;
    let a = analyze_ok(&obj_registry(), src);
    let k = &a.plan.kernels[0];
    assert_eq!(k.hoists.len(), 1);
    assert_eq!(k.hoists[0].ty, GpuTy::Float);
    assert!(k.records.is_empty());
    assert!(!k.needs_replay);
}

#[test]
fn per_iteration_random_values_are_recorded() {
    let src = r#"
math.randomseed(1)
for y = 0, 9 do
  for x = 0, 9 do
    obj.putpixel(x, y, math.random() * 255, 0, 0, 255)
  end
end"#;
    let a = analyze_ok(&obj_registry(), src);
    let k = &a.plan.kernels[0];
    assert_eq!(k.records.len(), 1);
    assert_eq!(k.slots_per_iteration, 1);
    assert!(k.needs_replay);
}

#[test]
fn random_count_inner_loops_use_capped_slots() {
    // ランダムな回数のループの中でのランダムな処理: 上限付きの固定スロットで記録する。
    let src = r#"
for x = 0, 9 do
  local v = 0
  for k = 1, math.random(1, 3) do
    v = v + math.random()
  end
  obj.putpixel(x, 0, v, 0, 0, 255)
end"#;
    let a = analyze_ok(&obj_registry(), src);
    let k = &a.plan.kernels[0];
    assert_eq!(k.records.len(), 2, "loop limit + value inside the loop");
    assert_eq!(k.record_loops.len(), 1);
    assert_eq!(k.record_loops[0].stride, 1);
    assert_eq!(k.record_loops[0].max_iterations, 16);
    assert_eq!(k.slots_per_iteration, 1 + 16);
}

#[test]
fn records_inside_while_loops_are_not_supported_yet() {
    let src = r#"
for x = 0, 9 do
  local n = 0
  while n < 3 do
    n = n + math.random()
  end
  obj.putpixel(x, 0, n, 0, 0, 255)
end"#;
    let codes = error_codes(&obj_registry(), src);
    assert!(codes.contains(&"cpu-value-unsupported-loop"), "{codes:?}");
}

#[test]
fn side_effects_under_gpu_conditions_are_errors() {
    let src = r#"
for x = 0, 9 do
  local r = obj.getpixel(x, 0)
  if r > 128 then print(x) end
end"#;
    let codes = error_codes(&obj_registry(), src);
    assert!(codes.contains(&"cpu-call-gpu-data"), "{codes:?}");
}

#[test]
fn cpu_side_effects_are_replayed() {
    let src = r#"
for x = 0, 9 do
  io.write(x, "\n")
  obj.putpixel(x, 0, 1, 1, 1, 1)
end"#;
    let a = analyze_ok(&obj_registry(), src);
    let k = &a.plan.kernels[0];
    assert!(k.needs_replay);
    assert!(!k.replay_leaves.is_empty());
}

#[test]
fn captures_and_externals() {
    let src = r#"
local k = 3
local mode = "add"
for x = 0, obj.w - 1 do
  if mode == "add" then
    obj.putpixel(x, 0, x * k, 0, 0, 255)
  end
end"#;
    let a = analyze_ok(&obj_registry(), src);
    let kern = &a.plan.kernels[0];
    // k は数値として捕捉、mode == "add" は文字列比較なので CPU で 1 回だけ計算(持ち出し)。
    assert!(kern.captures.values().any(|c| matches!(c.source, CaptureSource::Var(_)) && c.ty == GpuTy::Float));
    assert_eq!(kern.hoists.len(), 1);
    assert_eq!(kern.hoists[0].ty, GpuTy::Bool);
}

#[test]
fn local_arrays_and_captured_tables() {
    let src = r#"
local weights = {1, 2, 1}
for x = 1, 8 do
  local acc = 0
  local offs = {-1, 0, 1}
  for i = 1, #offs do
    acc = acc + obj.getpixel(x + offs[i], 0) * weights[i]
  end
  obj.putpixel(x, 0, acc / 4, 0, 0, 255)
end"#;
    let a = analyze_ok(&obj_registry(), src);
    let k = &a.plan.kernels[0];
    let cap = k.captures.values().next().unwrap();
    assert!(matches!(&cap.ty, GpuTy::Array(e, lua_gpu::kernel::types::ArrLen::Param) if **e == GpuTy::Float));
    assert!(!k.needs_replay);
}
