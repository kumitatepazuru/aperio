// 並列化の判定: 私有化、P の値の持ち越し、U の状態の再生、共有テーブルへの書き込み。

mod support;

use lua_gpu::kernel::VarRef;
use support::*;

#[test]
fn outer_locals_assigned_before_use_are_privatized() {
    // AviUtl のスクリプトでよくある、ループの外で宣言した local を使い回す書き方。
    let src = r#"
local r, g, b, a
for y = 0, 9 do
  for x = 0, 9 do
    r, g, b, a = obj.getpixel(x, y)
    obj.putpixel(x, y, b, g, r, a)
  end
end"#;
    let a = analyze_ok(&obj_registry(), src);
    let k = &a.plan.kernels[0];
    assert_eq!(k.dims.len(), 2);
    assert_eq!(k.privatized.len(), 4);
    assert!(!k.needs_replay);
}

#[test]
fn gpu_values_used_after_the_loop_block_parallelization() {
    let src = r#"
local last = 0
for x = 0, 9 do
  last = obj.getpixel(x, 0)
end
print(last)"#;
    let (_, d) = analyze(&obj_registry(), src);
    assert!(d.warnings().any(|w| w.code == "not-parallelized"), "{d}");
    assert!(d.errors().any(|e| e.code == "gpu-value-to-cpu"), "{d}");
}

#[test]
fn reads_before_assignment_block_privatization() {
    // 前の反復の値を読んでいる(反復どうしが独立していない)。
    let src = r#"
local prev = 0
for x = 0, 9 do
  obj.putpixel(x, 0, prev, prev, prev, 255)
  prev = obj.getpixel(x, 1)
end"#;
    let (_, d) = analyze(&obj_registry(), src);
    assert!(d.warnings().any(|w| w.code == "not-parallelized"), "{d}");
}

#[test]
fn cpu_state_is_replayed() {
    // U の値の持ち越し(カウンター)は CPU の再生で計算し、GPU は記録された値を読む。
    let src = r#"
local count = 0
for x = 0, 9 do
  count = count + 1
  obj.putpixel(x, 0, count, 0, 0, 255)
end
print(count)"#;
    let a = analyze_ok(&obj_registry(), src);
    let k = &a.plan.kernels[0];
    assert!(k.carried.iter().any(|v| matches!(v, VarRef::Local(_))));
    assert!(k.needs_replay);
    assert_eq!(k.records.len(), 1);
}

#[test]
fn gpu_values_cannot_be_written_to_shared_tables() {
    let src = r#"
local hist = {}
for x = 0, 9 do
  hist[x] = obj.getpixel(x, 0)
end"#;
    let (_, d) = analyze(&obj_registry(), src);
    assert!(d.warnings().any(|w| w.code == "not-parallelized"), "{d}");
    assert!(d.has_errors(), "{d}");
}
