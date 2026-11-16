//! LuaRuntime(コア)の遅延計測。合否判定はせず、結果を表示するだけ。
//!
//! `cargo test -p lua_runtime --release --test latency -- --ignored --nocapture --test-threads=1`
//!
//! コールバックは Rust のクロージャなので、Python の呼び出しコストは含まない
//! (Python 経由の値は tests/bench_latency.py で測る)。

use std::hint::black_box;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use lua_runtime::{Callback, LuaRuntime, Value, ValueType};

/// 1 回あたりの計測を行う項目の反復回数。
const SINGLE_ITERS: usize = 100_000;
/// `run` を含む項目の反復回数。
const RUN_ITERS: usize = 10_000;
/// 1 回の `run` の中で関数・プロシージャを呼ぶ回数。
const CALLS_PER_RUN: usize = 100_000;
/// 呼び出しコストの計測で `run` を繰り返す回数。
const CALL_REPEATS: usize = 30;

struct Stats {
    mean: f64,
    p50: f64,
    p99: f64,
}

fn stats(mut samples: Vec<f64>) -> Stats {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let pick = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    Stats {
        mean,
        p50: pick(0.5),
        p99: pick(0.99),
    }
}

fn print_row(label: &str, s: Stats) {
    println!(
        "| {label:<44} | {:>10.3} | {:>10.3} | {:>10.3} |",
        s.mean / 1000.0,
        s.p50 / 1000.0,
        s.p99 / 1000.0
    );
}

/// `f` を `iters` 回呼び、1 回ずつの所要時間(ns)を返す。
fn measure(iters: usize, mut f: impl FnMut()) -> Vec<f64> {
    for _ in 0..(iters / 10).max(10) {
        f();
    }
    (0..iters)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_nanos() as f64
        })
        .collect()
}

fn noop(ret: Value) -> Callback {
    Arc::new(move |args: &[Value]| {
        black_box(args);
        Ok(ret.clone())
    })
}

fn new_runtime() -> LuaRuntime {
    let mut rt = LuaRuntime::new();
    rt.add_variable("obj.x", ValueType::Float, Value::Float(100.0))
        .unwrap()
        .add_function(
            "obj.func",
            noop(Value::Float(1.0)),
            Some(ValueType::Float),
            vec![ValueType::Str, ValueType::Int, ValueType::Bool],
        )
        .unwrap()
        .add_procedure(
            "obj.rproc",
            noop(Value::Nil),
            true,
            vec![ValueType::Str, ValueType::Int, ValueType::Int],
        )
        .unwrap()
        .add_procedure(
            "obj.dproc",
            noop(Value::Nil),
            false,
            vec![ValueType::Str, ValueType::Int, ValueType::Int],
        )
        .unwrap();
    rt
}

/// `call` を `CALLS_PER_RUN` 回呼ぶスクリプトの 1 呼び出しあたりの時間(空ループとの差分)。
fn per_call(rt: &mut LuaRuntime, call: &str) -> Vec<f64> {
    let baseline = format!("for i = 1, {CALLS_PER_RUN} do end");
    let script = format!("for i = 1, {CALLS_PER_RUN} do {call} end");
    let base = stats(measure(CALL_REPEATS, || {
        rt.run(&baseline).unwrap();
    }))
    .p50;
    measure(CALL_REPEATS, || {
        rt.run(&script).unwrap();
    })
    .into_iter()
    .map(|t| (t - base) / CALLS_PER_RUN as f64)
    .collect()
}

#[test]
#[ignore = "遅延計測(手動実行)"]
fn latency() {
    let mut rt = new_runtime();

    println!();
    println!(
        "| {:<44} | {:>10} | {:>10} | {:>10} |",
        "item (Rust callback)", "mean [us]", "p50 [us]", "p99 [us]"
    );
    println!("|{:-<46}|{:->12}|{:->12}|{:->12}|", "", "", "", "");

    print_row(
        "set_variable",
        stats(measure(SINGLE_ITERS, || {
            rt.set_variable("obj.x", Value::Float(56.9)).unwrap();
        })),
    );
    print_row(
        "get_variable",
        stats(measure(SINGLE_ITERS, || {
            black_box(rt.get_variable("obj.x").unwrap());
        })),
    );
    print_row(
        "run(\"\")",
        stats(measure(RUN_ITERS, || {
            rt.run("").unwrap();
        })),
    );
    print_row(
        "set_variable -> run(small) -> get_variable",
        stats(measure(RUN_ITERS, || {
            rt.set_variable("obj.x", Value::Float(56.9)).unwrap();
            rt.run("obj.x = obj.x * 2").unwrap();
            black_box(rt.get_variable("obj.x").unwrap());
        })),
    );
    print_row(
        "function call (per call)",
        stats(per_call(&mut rt, "obj.func('a', i, true)")),
    );
    print_row(
        "realtime procedure call (per call)",
        stats(per_call(&mut rt, "obj.rproc('a', i, 2)")),
    );
    print_row(
        "deferred procedure, total (per call)",
        stats(per_call(&mut rt, "obj.dproc('a', i, 2)")),
    );

    // 遅延プロシージャを「スクリプト内でキューに積む」と「run 終了後に Rust から実行」に分ける。
    // スクリプトの最後でリアルタイム関数 mark() を呼び、その時刻を境目にする。
    let marker: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let m = Arc::clone(&marker);
    rt.add_function(
        "mark",
        Arc::new(move |_: &[Value]| {
            *m.lock().unwrap() = Some(Instant::now());
            Ok(Value::Nil)
        }),
        None,
        vec![],
    )
    .unwrap();
    let mut split = |script: &str| -> (Vec<f64>, Vec<f64>) {
        let mut enqueue = Vec::new();
        let mut flush = Vec::new();
        for i in 0..CALL_REPEATS + 3 {
            let t0 = Instant::now();
            rt.run(script).unwrap();
            let t1 = Instant::now();
            let tm = marker.lock().unwrap().take().unwrap();
            if i >= 3 {
                enqueue.push(tm.duration_since(t0).as_nanos() as f64);
                flush.push(t1.duration_since(tm).as_nanos() as f64);
            }
        }
        (enqueue, flush)
    };
    let (base_enqueue, base_flush) = split(&format!("for i = 1, {CALLS_PER_RUN} do end mark()"));
    let (enqueue, flush) = split(&format!(
        "for i = 1, {CALLS_PER_RUN} do obj.dproc('a', i, 2) end mark()"
    ));
    let base_enqueue = stats(base_enqueue).p50;
    let base_flush = stats(base_flush).p50;
    let per = |v: Vec<f64>, base: f64| -> Vec<f64> {
        v.into_iter()
            .map(|t| (t - base) / CALLS_PER_RUN as f64)
            .collect()
    };
    print_row(
        "deferred procedure, enqueue in Lua (per call)",
        stats(per(enqueue, base_enqueue)),
    );
    print_row(
        "deferred procedure, flush from Rust (per call)",
        stats(per(flush, base_flush)),
    );

    println!();
    println!(
        "(calls per run: {CALLS_PER_RUN}, repeats: {CALL_REPEATS}, single iters: {SINGLE_ITERS}, run iters: {RUN_ITERS})"
    );
}
