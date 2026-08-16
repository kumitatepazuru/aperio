import aperio_plugin
from aperio_plugin.plugin_base.generator_base import VideoGenerateParameters


def in_out_ramp(params: VideoGenerateParameters, in_seconds: float, out_seconds: float) -> tuple[float, bool]:
    """`イン`/`アウト`(秒)からオブジェクト内の進捗 g (0.0..1.0)を作る。
    `フェード`(exedit-inspect fade README §4)と`ワイプ`(同 wipe README §3)が
    共有するランプで、両端に向かって 1.0 から落ちる2本のランプの min。

    戻り値 `(g, from_out)` の `from_out` は「アウト側のランプが勝った」かどうか。
    ワイプが`反転(イン)`/`反転(アウト)`のどちらのチェックボックスを見るかを
    これで決める(wipe README §3-2)。フェードは g だけ使う。

    g >= 1.0 は「区間の内側 = 何もしない」を意味するので、呼び出し側は None を
    返してパス自体をスキップする(原作の `return 1`)。

    原作にはもう1つ `g <= 0 -> return 0`(オブジェクトを一切描かない)という出口が
    あるが、分子・分母の両方に +1 が付くこのランプでは t/u >= 0 である限り g は
    必ず正になるため到達しない(fade README §5)。
    """
    fps = aperio_plugin.store_manager.get_state().frame_state.fps
    in_frames = round(in_seconds * fps)
    out_frames = round(out_seconds * fps)

    # アイテム内のフレーム位置。item.end は排他境界なので end-1 が原作の
    # inclusive な終端フレームに相当する。
    t = params.frame_number - params.layer.start
    u = (params.layer.end - 1) - params.frame_number

    # イン側の `if (a < 4096)` は到達不能なガードなので省略し、min()1本に
    # まとめている(fade README §4)。アウト側は原作どおり厳密不等号の
    # `if (g > a)` にして、同値ならイン側が勝つ挙動を保つ。
    g = 1.0
    from_out = False
    if t < in_frames:
        g = min(g, (t + 1) / (in_frames + 1))
    if u < out_frames:
        a = (u + 1) / (out_frames + 1)
        if g > a:
            g = a
            from_out = True

    return g, from_out
