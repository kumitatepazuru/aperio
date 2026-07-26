import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

# 拡散ループの初期半径(px)・パス数・等比数列の指数。元のAviUtl版のディス
# アセンブル解析で確認された定数で、半径は 2px -> 拡散(生値)px まで6パスで
# 等比数列的に増加する: k = (拡散*0.5)^_DIFFUSION_EXPONENT とすると
# 2.0 * k^5 = 拡散(生値) になる。_DIFFUSION_EXPONENTは0.2で固定
# (6パス=5回の乗算なので1/(_DIFFUSION_PASSES-1)と等価だが、元のバイナリ
# 上でも0.2という独立した即値として埋め込まれているためそれに合わせる)。
_DIFFUSION_INITIAL_RADIUS = 2.0
_DIFFUSION_PASSES = 6
_DIFFUSION_EXPONENT = 0.2

# 「高速化」ON時の近似ぼかしの品質ノブ。縮小画像側のぼかし半径をこの値以下に
# 抑えることで、総半径によらず誤差が頭打ち(発光ピークの約1%以下)になる。
# 小さくするほど積極的に縮小して速くなるが誤差が増える(16=保守的, 8=積極的)。
# TODO: 設定で変えられるようにしたいよね
_FAST_BLUR_R_TARGET = 16


def _choose_downsample_factor(radius: int, r_target: int) -> int:
    """近似ぼかしの縮小率D(2冪)を返す。縮小画像側の半径がおよそ
    [r_target/2, r_target]、最低でも4以上に収まる最小のDを選ぶ。
    scratchpad/blur_error.py で誤差を実測した式と同一。"""
    if radius <= r_target:
        return 1
    factor = 1
    while radius / (factor * 2) >= r_target // 2 and factor * 2 <= radius:
        factor *= 2
    while radius // factor < 4 and factor > 1:
        factor //= 2
    return factor


class LuminousEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.luminous_effect"
        self.display_name = "発光"
        self.description = "Extracts bright areas, diffuses them with a multi-scale blur, and blends them back as a colored glow."

        current_dir = os.path.dirname(__file__)

        def load(path: str) -> str:
            with open(path, "r") as f:
                return f.read()

        self.threshold_shader = PyCompiledWgsl(
            "luminous_threshold", load(os.path.join(current_dir, "threshold.wgsl")), aperio_plugin.image_generator, None
        )
        self.curve_shader = PyCompiledWgsl(
            "luminous_curve", load(os.path.join(current_dir, "curve.wgsl")), aperio_plugin.image_generator, None
        )
        self.expand_shader = PyCompiledWgsl(
            "luminous_expand", load(os.path.join(current_dir, "expand.wgsl")), aperio_plugin.image_generator, None
        )
        self.merge_shader = PyCompiledWgsl(
            "luminous_merge", load(os.path.join(current_dir, "merge.wgsl")), aperio_plugin.image_generator, None
        )
        self.box_blur_h_shader = PyCompiledWgsl(
            "luminous_box_blur_h", load(os.path.join(current_dir, "box_blur_h.wgsl")), aperio_plugin.image_generator, None
        )
        self.box_blur_v_shader = PyCompiledWgsl(
            "luminous_box_blur_v", load(os.path.join(current_dir, "box_blur_v.wgsl")), aperio_plugin.image_generator, None
        )
        self.select_shader = PyCompiledWgsl(
            "luminous_select", load(os.path.join(current_dir, "select.wgsl")), aperio_plugin.image_generator, None
        )
        self.accumulate_saturating_shader = PyCompiledWgsl(
            "luminous_accumulate_saturating",
            load(os.path.join(current_dir, "accumulate_saturating.wgsl")),
            aperio_plugin.image_generator,
            None,
        )
        self.combined_init_shader = PyCompiledWgsl(
            "luminous_combined_init",
            load(os.path.join(current_dir, "combined_init.wgsl")),
            aperio_plugin.image_generator,
            None,
        )
        self.accumulate_chroma_shader = PyCompiledWgsl(
            "luminous_accumulate_chroma",
            load(os.path.join(current_dir, "accumulate_chroma.wgsl")),
            aperio_plugin.image_generator,
            None,
        )
        self.accumulate_luma_combined_shader = PyCompiledWgsl(
            "luminous_accumulate_luma_combined",
            load(os.path.join(current_dir, "accumulate_luma_combined.wgsl")),
            aperio_plugin.image_generator,
            None,
        )
        self.reconstruct_shader = PyCompiledWgsl(
            "luminous_reconstruct", load(os.path.join(current_dir, "reconstruct.wgsl")), aperio_plugin.image_generator, None
        )
        # 「高速化」ON時の近似ぼかし(縮小ピラミッド)用。
        self.downsample_shader = PyCompiledWgsl(
            "luminous_downsample", load(os.path.join(current_dir, "downsample.wgsl")), aperio_plugin.image_generator, None
        )
        self.upsample_shader = PyCompiledWgsl(
            "luminous_upsample", load(os.path.join(current_dir, "upsample.wgsl")), aperio_plugin.image_generator, None
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=None,
            max_frame=None,
            min_frame=None,
            structure=[
                RequestStructureParameter.Int(
                    id="strength",
                    title="強さ",
                    default_value=20,
                    min=0,
                    max=200,
                ),
                RequestStructureParameter.Int(
                    id="diffusion",
                    title="拡散",
                    default_value=50,
                    min=0,
                    max=800
                ),
                RequestStructureParameter.Int(
                    id="threshold",
                    title="しきい値",
                    default_value=70,
                    min=0,
                    max=200,
                ),
                RequestStructureParameter.Int(
                    id="diffusion_speed",
                    title="拡散速度",
                    default_value=0,
                    min=0,
                    max=60
                ),
                RequestStructureParameter.Color(
                    id="color",
                    title="光色",
                    default_value=(1.0, 0.0, 1.0, 1.0),
                    use_alpha=False,
                ),
                # 光色「指定なし」(実機のex_data bit24セット=既定)。オンにすると
                # 光色を無視し、元画素自身の色差で発光する(輝度は抽出量そのまま)。
                # 実機の工場出荷既定はこちらだが、移植版の既存デフォルト(光色使用)を
                # 壊さないよう既定はオフにしてある。
                RequestStructureParameter.Bool(
                    id="use_source_color",
                    title="光色: 元画像の色を使う",
                    default_value=False,
                ),
                # ONにするとぼかしを近似(縮小ピラミッド)で高速化する。平坦部の
                # 発色は厳密なままだが、輪郭付近の遷移帯だけ実機基準からわずかに
                # ずれる(拡散が大きいほど速く、誤差は最大でも発光ピークの約1%)。
                RequestStructureParameter.Bool(
                    id="fast_mode",
                    title="高速化",
                    default_value=True,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        strength_ui = max(0, min(200, args.get("strength", 20)))
        diffusion_ui = max(0, min(800, args.get("diffusion", 50)))
        threshold_ui = max(0, min(200, args.get("threshold", 70)))
        diffusion_speed_ui = max(0, min(60, args.get("diffusion_speed", 0)))
        color = args.get("color", (1.0, 0.0, 1.0, 1.0))

        # 強さ・しきい値: 元のAviUtl版はY値が0~4096の固定小数点で、
        # fixed_gain = raw*4096/1000 (raw=表示値*10、4096でクランプ)を
        # (y-threshold)*fixed_gain>>12 という形で使っている。この式全体を
        # 4096で割って0.0~1.0の正規化float空間に落とすと、fixed_gain/4096 =
        # raw/1000 = 表示値/100 となり4096が両辺で約分されて消えるため、
        # ここでは単純に表示値/100をgain(乗算率)として使うだけでよい。
        # 強さが100%を超えた分もfixed_gainではなく別枠のoverflow(同じく
        # /4096した後の値)として加算し、しきい値ぎりぎりの暗いピクセルまで
        # 底上げする元の挙動を再現する。
        strength_frac = strength_ui / 100.0
        gain = min(strength_frac, 1.0)
        overflow = max(0.0, strength_frac - 1.0)
        threshold_frac = threshold_ui / 100.0

        if gain <= 0.0 and overflow <= 0.0:
            return None

        width, height = params.width, params.height

        # 拡散: 半径2pxから拡散(生値)pxまで、6パスで等比数列的に増加させる。
        # 2.0 * k^5 = 拡散(生値) になるよう k = (拡散*0.5)^0.2 を選ぶ。
        k = (diffusion_ui * 0.5) ** _DIFFUSION_EXPONENT
        radii = []
        accum = _DIFFUSION_INITIAL_RADIUS
        for _ in range(_DIFFUSION_PASSES):
            radii.append(round(accum))
            accum *= k
        max_radius = max(radii)

        # 発光がキャンバス外ににじみ出す分だけ、最大半径ぶんを一度だけ拡張する。
        # 以降の6パスはこの固定サイズのキャンバス内で処理する。
        new_width = width + 2 * max_radius
        new_height = height + 2 * max_radius
        # 半径は方向別にクランプする(実機は垂直パスを h/2-1、水平パスを w/2-1 で
        # 個別にクランプする。README手順4)。拡張後キャンバスの幅・高さで割るので、
        # 非正方キャンバスでは縦横で別々の上限になる。
        cap_h = max(0, new_width // 2 - 1)   # 水平パス(box_blur_h)の半径上限
        cap_v = max(0, new_height // 2 - 1)  # 垂直パス(box_blur_v)の半径上限

        # 注: 強さ/しきい値は UI*10=raw(ui/100 が raw/1000 に一致)として扱う一方、
        # 拡散だけは UI をそのまま raw として半径計算に渡している。README手順10の
        # 通り、この UI↔raw の対応関係はバイナリからは確定できないため、視覚的に
        # 合っている限り意図的にこの不一致のままにしてある。

        color_r, color_g, color_b = color[0], color[1], color[2]
        # 光色 -> BT.601 の輝度/色差偏差(光色指定モードで使う定数係数)。指定なし
        # モードでは threshold.wgsl 側が元画素ごとに算出するのでここの値は使われない。
        luma_color = 0.299 * color_r + 0.587 * color_g + 0.114 * color_b
        cr_dev = (color_r - luma_color) / 1.402000
        cb_dev = (color_b - luma_color) / 1.772000
        use_source_chroma = 1 if args.get("use_source_color", False) else 0
        fast = bool(args.get("fast_mode", False))

        # ぼかし(box_blur_h/v)が負値を max(0,x) で潰すため、色差(r/g)には常に正に
        # なるよう十分大きい定数offsetを足しておき、蓄積時に差し引く。輝度(b)は
        # 非負なのでoffset不要。
        chroma_offset = 8.0

        threshold_params = struct.pack(
            "fffifff", gain, overflow, threshold_frac, use_source_chroma, cr_dev, cb_dev, luma_color
        )
        expand_params = struct.pack(
            "iiiiffff", max_radius, max_radius, new_width, new_height, 0.0, 0.0, 0.0, 0.0
        )
        # combined_init通過後は色差(r/g)がoffset込み・輝度(b)が0で外側を埋め、
        # a=1(box_blurのプリマルチプライドno-op用)。
        combined_expand_params = struct.pack(
            "iiiiffff", max_radius, max_radius, new_width, new_height,
            chroma_offset, chroma_offset, 0.0, 1.0,
        )

        base_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.expand_shader, expand_params, new_width, new_height)

        # 明部抽出は1回だけ。出力は {r=amount*Cr係数, g=amount*Cb係数,
        # b=amount*輝度係数, a=amount} で、以降 combined_init だけがこれを読む。
        threshold_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
            self.threshold_shader, threshold_params, width, height
        )

        # 拡散速度: >0のときだけ、ぼかし前後で指数/対数カーブの往復変換を挟む
        # (0の場合は実機でもこの変換自体がスキップされ、代わりに飽和付き加算に
        # なる。README手順4・5)。
        use_diffusion_curve = diffusion_speed_ui > 0
        curve_base = 1.0
        if use_diffusion_curve:
            diffusion_speed_clamped = max(1, min(100, diffusion_speed_ui))
            curve_base = 1.0 + diffusion_speed_clamped * 0.001

        def select_branch(index: int) -> gpu_util.PyImageGenerateBuilder:
            return gpu_util.PyImageGenerateBuilder().add_wgsl(
                self.select_shader, struct.pack("i", index), new_width, new_height
            )

        def exact_box_branch(r_h: int, r_v: int) -> gpu_util.PyImageGenerateBuilder:
            # 方向別にクランプした半径で水平→垂直のボックスぼかし。両方向とも
            # 半径0のときだけ素通し(片方だけ0なら radius=0=恒等タップで安全)。
            if r_h <= 0 and r_v <= 0:
                return select_branch(0)
            return (
                gpu_util.PyImageGenerateBuilder()
                .add_wgsl(
                    self.box_blur_h_shader, struct.pack("iii", r_h, new_width, new_height), new_width, new_height
                )
                .add_wgsl(
                    self.box_blur_v_shader, struct.pack("iii", r_v, new_width, new_height), new_width, new_height
                )
            )

        def blur_branch_for(radius: int) -> gpu_util.PyImageGenerateBuilder:
            r_h = min(radius, cap_h)
            r_v = min(radius, cap_v)
            # 「高速化」OFF(既定)は現行どおりの厳密ボックスぼかし(挙動完全不変)。
            factor = _choose_downsample_factor(radius, _FAST_BLUR_R_TARGET) if fast else 1
            if factor <= 1:
                return exact_box_branch(r_h, r_v)

            # 「高速化」ON かつ大半径: 縮小 -> 小画像でボックス -> バイリニア拡大。
            # タップ数が factor^2 分の1になる。誤差は輪郭付近の遷移帯だけ(平坦部は
            # 縮小/拡大とも線形なので恒等 = 厳密一致)。
            small_w = (new_width + factor - 1) // factor
            small_h = (new_height + factor - 1) // factor
            rs = max(1, round(radius / factor))
            rs_h = min(rs, max(0, small_w // 2 - 1))
            rs_v = min(rs, max(0, small_h // 2 - 1))
            return (
                gpu_util.PyImageGenerateBuilder()
                .add_wgsl(self.downsample_shader, struct.pack("iii", factor, small_w, small_h), small_w, small_h)
                .add_wgsl(self.box_blur_h_shader, struct.pack("iii", rs_h, small_w, small_h), small_w, small_h)
                .add_wgsl(self.box_blur_v_shader, struct.pack("iii", rs_v, small_w, small_h), small_w, small_h)
                .add_wgsl(self.upsample_shader, struct.pack("iii", factor, new_width, new_height), new_width, new_height)
            )

        # --- 両経路共通の入口: 抽出結果を6パスぼかしに乗せる形へ整える ---
        # combined_init は色差(r/g)にoffsetを足し、輝度(b)を拡散速度>0のときだけ
        # 指数カーブに通す(apply_curveフラグ)。threshold_branch(抽出結果)から
        # 続ける必要がある(まっさらなbuilder()から始めると入力が元フレームその
        # ものになり、抽出を経ないまま生ピクセルを渡す重大なバグになる)。
        combined_init_params = struct.pack(
            "ffi", curve_base, chroma_offset, 1 if use_diffusion_curve else 0
        )
        combined_cont = threshold_branch.add_wgsl(
            self.combined_init_shader, combined_init_params, width, height
        )
        combined_cont = combined_cont.add_wgsl(self.expand_shader, combined_expand_params, new_width, new_height)

        # 6パスのぼかしは前段の出力を次段が再びぼかす「連鎖」で、各パスのぼかし
        # 結果を毎回アキュムレータへ蓄積する(AviUtl版が垂直パスのたびに蓄積
        # バッファへ書き込むのと同じ。結合則が成り立つので毎パス足し込んでよい)。
        if not use_diffusion_curve:
            # 拡散速度0: 輝度・色差を1枚 {r=Σcr, g=Σcb, a=Σy} の飽和アキュムレータで
            # 結合して蓄積する(輝度が飽和すると色差が退色していく実機の挙動。
            # README手順4)。状態は [chain, accum] の2枚。
            state_len = 1
            for radius in radii:
                passthrough = [select_branch(1)] if state_len > 1 else []
                combined_cont = combined_cont.add_parallel_wgsl([blur_branch_for(radius)] + passthrough)
                # state = [new_chain] (初回) または [new_chain, old_accum] (2回目以降)

                if state_len == 1:
                    # 初回はambient state = [new_chain] の1枚だけ。is_first=1で
                    # inputTex[0](新チェーン)だけを読み、旧蓄積は0扱いにする。
                    accumulate_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
                        self.accumulate_saturating_shader, struct.pack("fi", chroma_offset, 1), new_width, new_height
                    )
                else:
                    # accumulate_saturatingは[新チェーン, 旧蓄積]の2枚を前提に
                    # inputTex[0]/[1]を読むので、その2枚だけを詰め直してから呼ぶ。
                    accumulate_branch = (
                        gpu_util.PyImageGenerateBuilder()
                        .add_parallel_wgsl([select_branch(0), select_branch(1)])
                        .add_wgsl(self.accumulate_saturating_shader, struct.pack("fi", chroma_offset, 0), new_width, new_height)
                    )
                combined_cont = combined_cont.add_parallel_wgsl([select_branch(0), accumulate_branch])
                state_len = 2

            # state = [chain(不要), accum]。accumを両入力(inputTex[0].a=Σy,
            # inputTex[1].r/g=Σcr/Σcb)としてreconstructへ渡す。
            glow_branch = combined_cont.add_parallel_wgsl(
                [select_branch(1), select_branch(1)]
            ).add_wgsl(self.reconstruct_shader, None, new_width, new_height)
        else:
            # 拡散速度>0: 輝度(exp/log空間の単純加算)と色差(毎パス2032/4096クランプ)は
            # 蓄積方式が異なるため独立したアキュムレータを持つ。輝度・色差は同じ
            # カーネル重みの単純重み付き平均でチャンネル間の相互作用が無いため、
            # 1枚のRGBAに r=Cr偏差, g=Cb偏差, b=輝度量(カーブ後), a=1.0 を詰めて
            # 同じチェーンを1回回すだけでよい。状態は [chain, luma_accum,
            # chroma_accum] の3枚。
            combined_state_len = 1
            for radius in radii:
                combined_passthrough = [select_branch(1), select_branch(2)] if combined_state_len > 1 else []
                combined_cont = combined_cont.add_parallel_wgsl([blur_branch_for(radius)] + combined_passthrough)
                # state = [new_chain] (初回) または [new_chain, old_luma_accum, old_chroma_accum]

                if combined_state_len == 1:
                    # 初回はambient state = [new_chain] の1枚だけ。
                    luma_accum_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
                        self.accumulate_luma_combined_shader, struct.pack("i", 1), new_width, new_height
                    )
                    chroma_accum_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
                        self.accumulate_chroma_shader, struct.pack("fi", chroma_offset, 1), new_width, new_height
                    )
                else:
                    # accumulate_*は[新チェーン, 旧蓄積]の2枚を前提にinputTex[0]/[1]を
                    # 読むため、輝度用・色差用それぞれ必要な2枚だけを詰め直して呼ぶ。
                    luma_accum_branch = (
                        gpu_util.PyImageGenerateBuilder()
                        .add_parallel_wgsl([select_branch(0), select_branch(1)])
                        .add_wgsl(self.accumulate_luma_combined_shader, struct.pack("i", 0), new_width, new_height)
                    )
                    chroma_accum_branch = (
                        gpu_util.PyImageGenerateBuilder()
                        .add_parallel_wgsl([select_branch(0), select_branch(2)])
                        .add_wgsl(
                            self.accumulate_chroma_shader, struct.pack("fi", chroma_offset, 0), new_width, new_height
                        )
                    )
                combined_cont = combined_cont.add_parallel_wgsl(
                    [select_branch(0), luma_accum_branch, chroma_accum_branch]
                )
                combined_state_len = 2

            # state = [chain(不要), luma_accum, chroma_accum]。輝度だけカーブ逆変換で
            # a=y_finalに戻し、色差はそのままreconstructへ渡す。
            finalize_luma = gpu_util.PyImageGenerateBuilder().add_wgsl(
                self.select_shader, struct.pack("i", 1), new_width, new_height
            ).add_wgsl(self.curve_shader, struct.pack("fi", curve_base, 1), new_width, new_height)
            finalize_chroma = gpu_util.PyImageGenerateBuilder().add_wgsl(
                self.select_shader, struct.pack("i", 2), new_width, new_height
            )
            glow_branch = (
                combined_cont
                .add_parallel_wgsl([finalize_luma, finalize_chroma])
                .add_wgsl(self.reconstruct_shader, None, new_width, new_height)
            )

        builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_parallel_wgsl([base_branch, glow_branch])
            .add_wgsl(self.merge_shader, None, new_width, new_height)
        )

        return GeneratorBuilderReturn(builder, ItemResult(new_width, new_height))
