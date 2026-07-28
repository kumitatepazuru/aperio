import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

# `通常`形状のカスケード(exedit-inspect glow README §5.1)。半径は`拡散`をこれらの
# 除数で割ったもの、strengthは`強さ`にこの倍率を掛けたもの(強さ*6から始まり毎パス半分)。
_NORMAL_DIVISORS = (8, 6, 4, 2)
_NORMAL_MULTS = (6.0, 3.0, 1.5, 0.75)

# `形状`コンボの各選択肢が使う方向ベクトルの集合。斜め方向は中心画素から±両方向へ
# サンプルする1回のループで対角線全体をカバーするため、実機の6ワーカーは4方向
# (横・縦・`\`・`/`)へ集約できる(README §5)。
_SHAPE_DIRECTIONS = {
    "line_h": [(1, 0)],
    "line_v": [(0, 1)],
    "cross4": [(1, 0), (0, 1)],
    "cross4_diag": [(1, 1), (1, -1)],
    "cross8": [(1, 0), (0, 1), (1, 1), (1, -1)],
}


class GlowEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.glow_effect"
        self.display_name = "グロー"
        self.description = "Extracts bright areas and spreads them as directional glow streaks or a soft cascade blur."

        current_dir = os.path.dirname(__file__)
        common_dir = os.path.join(current_dir, "..", "common")
        color_module = gpu_util.create_composable_module(os.path.join(common_dir, "lib", "color.wgsl"))

        def load(path: str) -> str:
            with open(path, "r") as f:
                return f.read()

        self.extract_shader = PyCompiledWgsl.compose_new(
            "glow_extract",
            [color_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "extract.wgsl")),
            aperio_plugin.image_generator,
        )
        self.box_average_dir_shader = PyCompiledWgsl(
            "glow_box_average_dir", load(os.path.join(current_dir, "box_average_dir.wgsl")), aperio_plugin.image_generator, None
        )
        self.line_accumulate_shader = PyCompiledWgsl.compose_new(
            "glow_line_accumulate",
            [color_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "line_accumulate.wgsl")),
            aperio_plugin.image_generator,
        )
        self.composite_shader = PyCompiledWgsl.compose_new(
            "glow_composite",
            [color_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "composite.wgsl")),
            aperio_plugin.image_generator,
        )
        self.select_shader = PyCompiledWgsl(
            "glow_select", load(os.path.join(current_dir, "select.wgsl")), aperio_plugin.image_generator, None
        )
        self.expand_shader = PyCompiledWgsl(
            "expand", load(os.path.join(common_dir, "expand.wgsl")), aperio_plugin.image_generator, None
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
                    default_value=40,
                    min=0,
                    max=400,
                ),
                RequestStructureParameter.Int(
                    id="diffusion",
                    title="拡散",
                    default_value=30,
                    suffix="px",
                    min=0,
                    max=200,
                ),
                RequestStructureParameter.Int(
                    id="threshold",
                    title="しきい値",
                    default_value=40,
                    min=0,
                    max=200,
                ),
                RequestStructureParameter.Int(
                    id="blur",
                    title="ぼかし",
                    default_value=1,
                    suffix="px",
                    min=0,
                    max=50,
                ),
                RequestStructureParameter.List(
                    id="shape",
                    title="形状",
                    values={
                        "normal": "通常",
                        "cross4": "クロス(4本)",
                        "cross4_diag": "クロス(4本斜め)",
                        "cross8": "クロス(8本)",
                        "line_h": "ライン(横)",
                        "line_v": "ライン(縦)",
                    },
                    default_value="normal",
                ),
                RequestStructureParameter.Color(
                    id="color",
                    title="光色",
                    default_value=(1.0, 1.0, 1.0, 1.0),
                    use_alpha=False,
                ),
                RequestStructureParameter.Bool(
                    id="use_source_color",
                    title="光色: 元画像の色を使う",
                    default_value=True,
                ),
                RequestStructureParameter.Bool(
                    id="light_only",
                    title="光成分のみ",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        strength_ui = max(0, min(400, args.get("strength", 40)))
        diffusion_ui = max(0, min(200, args.get("diffusion", 30)))
        threshold_ui = max(0, min(200, args.get("threshold", 40)))
        blur_radius = max(0, min(50, args.get("blur", 1)))
        shape: str = args.get("shape", "normal")
        color = args.get("color", (1.0, 1.0, 1.0, 1.0))
        use_source_color = bool(args.get("use_source_color", True))
        light_only = bool(args.get("light_only", False))

        # 強さ0または拡散0は実機と同じく即return(README: fp->func_procの2つの早期return)。
        if strength_ui == 0 or diffusion_ui == 0:
            return None

        w, h = params.width, params.height

        # 強さ・しきい値はAviUtl側の「表示スケール」の数値をそのままIntにしてあるので、
        # ここで元の raw / 正規化フラクションへ戻す(README §2)。
        strength_raw = strength_ui * 10
        threshold_frac = threshold_ui / 100.0

        # 拡散はrx・ryの両方に生値のまま使われる(README §2)ので実質1本の半径。
        # キャンバスが確保できるテクスチャの最大辺長を超えないよう安全にクランプする。
        max_dim = aperio_plugin.image_generator.maximum_texture_size
        diffusion = max(0, min(diffusion_ui, (max_dim - w) // 2, (max_dim - h) // 2))
        if diffusion == 0:
            return None

        nw, nh = w + 2 * diffusion, h + 2 * diffusion

        def select_branch(index: int) -> gpu_util.PyImageGenerateBuilder:
            return gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", index), nw, nh)

        def box_average_params(radius: int, step_x: int, step_y: int) -> bytes:
            return struct.pack("iiiii", radius, step_x, step_y, nw, nh)

        def line_accumulate_params(radius: int, step_x: int, step_y: int, gain: float, is_first: bool) -> bytes:
            return struct.pack("iiifiii", radius, step_x, step_y, gain, 1 if is_first else 0, nw, nh)

        # --- 明部抽出 -> キャンバス拡張(README §3, §7) ---
        extract_params = struct.pack(
            "fifff", threshold_frac, 1 if use_source_color else 0, color[0], color[1], color[2]
        )
        ext_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.extract_shader, extract_params, w, h)
        expand_params = struct.pack("iiiiffff", diffusion, diffusion, nw, nh, 0.0, 0.0, 0.0, 0.0)
        glow_chain = ext_branch.add_wgsl(self.expand_shader, expand_params, nw, nh)

        # --- 形状ごとの蓄積(README §5) ---
        if shape == "normal":
            # 蓄積の初期シードは拡張済み抽出結果そのもの(垂直パスの最初の読み取り元
            # としてのみ使い、水平加算の初回(is_first)でクリアされる)。
            for i, (divisor, mult) in enumerate(zip(_NORMAL_DIVISORS, _NORMAL_MULTS)):
                r = diffusion // divisor
                gain = strength_raw * mult / 16384.0
                v_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
                    self.box_average_dir_shader, box_average_params(r, 0, 1), nw, nh
                )
                glow_chain = glow_chain.add_parallel_wgsl([v_branch, select_branch(0)])
                glow_chain = glow_chain.add_wgsl(
                    self.line_accumulate_shader, line_accumulate_params(r, 1, 0, gain, i == 0), nw, nh
                )
        else:
            directions = _SHAPE_DIRECTIONS.get(shape, _SHAPE_DIRECTIONS["cross8"])
            is_first = True
            for dx, dy in directions:
                # 半径は r, r/2, r/4、strength倍率は 1, 2, 4(README §5.2: 3スケールの
                # 合計利得がほぼ等しくなる組み合わせ)。
                for radius_val, mult in ((diffusion, 1.0), (diffusion // 2, 2.0), (diffusion // 4, 4.0)):
                    gain = strength_raw * mult / 16384.0
                    accumulate_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
                        self.line_accumulate_shader, line_accumulate_params(radius_val, dx, dy, gain, is_first), nw, nh
                    )
                    glow_chain = glow_chain.add_parallel_wgsl([select_branch(0), accumulate_branch])
                    is_first = False
            # state = [source_const, accum] -> accum(index=1)だけを残す
            glow_chain = glow_chain.add_wgsl(self.select_shader, struct.pack("i", 1), nw, nh)

        # --- 仕上げ`ぼかし`(README §6: カーネル幅で割る素のボックス平均を2ラウンド) ---
        if blur_radius > 0:
            for _ in range(2):
                glow_chain = glow_chain.add_wgsl(
                    self.box_average_dir_shader, box_average_params(blur_radius, 0, 1), nw, nh
                )
                glow_chain = glow_chain.add_wgsl(
                    self.box_average_dir_shader, box_average_params(blur_radius, 1, 0), nw, nh
                )

        # --- 最終合成(README §8) ---
        base_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(self.expand_shader, expand_params, nw, nh)
        builder = gpu_util.PyImageGenerateBuilder().add_parallel_wgsl([base_branch, glow_chain]).add_wgsl(
            self.composite_shader, struct.pack("i", 1 if light_only else 0), nw, nh
        )

        return GeneratorBuilderReturn(builder, ItemResult(nw, nh))
