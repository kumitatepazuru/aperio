"""LuaRuntime の Python 経由の遅延計測。合否判定はせず、結果を表示するだけ。

    uv run python plugins/lua/tests/bench_latency.py

Python のコールバックと pyo3 の境界を含めた、実使用に近い値を測る
(コールバックが Rust のクロージャの場合は tests/latency.rs)。
release ビルドの lua_runtime がインストールされている前提。
"""

import statistics
import time
from collections.abc import Callable

from lua_runtime import LuaRuntime

SINGLE_ITERS = 100_000
RUN_ITERS = 10_000
CALLS_PER_RUN = 100_000
CALL_REPEATS = 30


def stats(samples: list[float]) -> tuple[float, float, float]:
    s = sorted(samples)
    return statistics.fmean(s), s[round((len(s) - 1) * 0.5)], s[round((len(s) - 1) * 0.99)]


def row(label: str, samples: list[float]) -> None:
    mean, p50, p99 = stats(samples)
    print(f"| {label:<46} | {mean / 1000:>10.3f} | {p50 / 1000:>10.3f} | {p99 / 1000:>10.3f} |")


def measure(iters: int, f: Callable[[], object]) -> list[float]:
    for _ in range(max(iters // 10, 10)):
        f()
    out = []
    now = time.perf_counter_ns
    for _ in range(iters):
        t = now()
        f()
        out.append(float(now() - t))
    return out


def func(s: str, i: int, b: bool) -> float:
    return 1.0


def proc(s: str, i: int, j: int) -> None:
    pass


marker: list[int] = []


def mark() -> None:
    marker.append(time.perf_counter_ns())


def per_call(rt: LuaRuntime, call: str) -> list[float]:
    baseline = f"for i = 1, {CALLS_PER_RUN} do end"
    script = f"for i = 1, {CALLS_PER_RUN} do {call} end"
    base = stats(measure(CALL_REPEATS, lambda: rt.run(baseline)))[1]
    return [(t - base) / CALLS_PER_RUN for t in measure(CALL_REPEATS, lambda: rt.run(script))]


def split(rt: LuaRuntime, script: str) -> tuple[list[float], list[float]]:
    """スクリプト末尾の mark()(リアルタイム)の時刻で、スクリプト実行と遅延プロシージャ実行に分ける。"""
    enqueue, flush = [], []
    for i in range(CALL_REPEATS + 3):
        marker.clear()
        t0 = time.perf_counter_ns()
        rt.run(script)
        t1 = time.perf_counter_ns()
        if i >= 3:
            enqueue.append(float(marker[0] - t0))
            flush.append(float(t1 - marker[0]))
    return enqueue, flush


def main() -> None:
    rt = (
        LuaRuntime()
        .add_variable("obj.x", float, 100.0)
        .add_function("obj.func", func, float, str, int, bool)
        .add_procedure("obj.rproc", proc, True, str, int, int)
        .add_procedure("obj.dproc", proc, False, str, int, int)
        .add_function("mark", mark, None)
    )

    print()
    print(f"| {'item (Python callback)':<46} | {'mean [us]':>10} | {'p50 [us]':>10} | {'p99 [us]':>10} |")
    print(f"|{'':-<48}|{'':->12}|{'':->12}|{'':->12}|")

    row("set_variable", measure(SINGLE_ITERS, lambda: rt.set_variable("obj.x", 56.9)))
    row("get_variable", measure(SINGLE_ITERS, lambda: rt.get_variable("obj.x")))
    row('run("")', measure(RUN_ITERS, lambda: rt.run("")))
    row(
        "set_variable -> run(small) -> get_variable",
        measure(RUN_ITERS, lambda: rt.set_variable("obj.x", 56.9).run("obj.x = obj.x * 2").get_variable("obj.x")),
    )
    row("function call (per call)", per_call(rt, "obj.func('a', i, true)"))
    row("realtime procedure call (per call)", per_call(rt, "obj.rproc('a', i, 2)"))
    row("deferred procedure, total (per call)", per_call(rt, "obj.dproc('a', i, 2)"))

    base_enqueue, base_flush = split(rt, f"for i = 1, {CALLS_PER_RUN} do end mark()")
    enqueue, flush = split(rt, f"for i = 1, {CALLS_PER_RUN} do obj.dproc('a', i, 2) end mark()")
    be, bf = stats(base_enqueue)[1], stats(base_flush)[1]
    row("deferred procedure, enqueue in Lua (per call)", [(t - be) / CALLS_PER_RUN for t in enqueue])
    row("deferred procedure, flush from Rust (per call)", [(t - bf) / CALLS_PER_RUN for t in flush])

    print()
    print(f"(calls per run: {CALLS_PER_RUN}, repeats: {CALL_REPEATS}, single iters: {SINGLE_ITERS}, run iters: {RUN_ITERS})")


if __name__ == "__main__":
    main()
