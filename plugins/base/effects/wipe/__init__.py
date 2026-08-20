import struct

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information, pack_box_blur_dir_params, split_radius
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader
from ...common.timeline import in_out_ramp

# 組み込み5パターン(README §4 の ex_data.type)。値がそのまま wipe_mask.wgsl の
# PATTERN_* 定数になる。
_PATTERNS = {
    "circle": 0,
    "square": 1,
    "clock": 2,
    "horizontal": 3,
    "vertical": 4,
}

# TODO: カスタム画像パターン(README §8)は未実装。
#   原作は exedit.auf と同じフォルダの `transition\*.png`を読み込むが、UI上での管理をできるようにしたいので別のときに実装をしたい
# TODO: UIの実装が可能なプラグインシステムの作成


class WipeEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.wipe_effect"
        self.display_name = "ワイプ"
        self.description = "Reveals or hides the object with a geometric pattern that sweeps in/out near the start/end of its duration."

        current_dir, common_dir = effect_dirs(__file__)
        blur_module = lib_module(common_dir, "blur")

        self.mask_shader = shared_shader("wipe_mask", current_dir, "wipe_mask.wgsl")
        # box_blur_dir: README §6 が「`ぼかし`エフェクト本体と同じ半径分割規則」と
        # 明記している共通の箱ぼかし。blur.py/sharp.py と同じ名前・フォーマット
        # フロアでコンパイルしてパイプラインキャッシュを共有する。
        self.box_blur_dir_shader = compose_common_shader(
            "box_blur_dir", [blur_module], common_dir, "box_blur_dir.wgsl",
            min_output_format=gpu_util.WrappedImagePixelFormat.Rgba32Float,
        )
        self.composite_shader = shared_shader("wipe_composite", current_dir, "wipe_composite.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="wipe_in",
                    title="イン",
                    default_value=0.5,
                    suffix="s",
                    min=0.0,
                    max=10.0,
                ),
                RequestStructureParameter.Float(
                    id="wipe_out",
                    title="アウト",
                    default_value=0.5,
                    suffix="s",
                    min=0.0,
                    max=10.0,
                ),
                RequestStructureParameter.Int(
                    id="blur",
                    title="ぼかし",
                    default_value=2,
                    suffix="px",
                    min=0,
                    max=100,
                ),
                RequestStructureParameter.Bool(
                    id="invert_in",
                    title="反転(イン)",
                    default_value=False,
                ),
                RequestStructureParameter.Bool(
                    id="invert_out",
                    title="反転(アウト)",
                    default_value=False,
                ),
                RequestStructureParameter.List(
                    id="pattern",
                    title="ワイプ",
                    values={
                        "circle": "円",
                        "square": "四角",
                        "clock": "時計",
                        "horizontal": "横",
                        "vertical": "縦",
                    },
                    default_value="circle",
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        wipe_in = args.get("wipe_in", 0.5)
        wipe_out = args.get("wipe_out", 0.5)
        # blurはsplit_radius経由でカーネル半径に直結するため、下限0は維持する。
        blur = max(0, round(args.get("blur", 2)))
        invert_in: bool = args.get("invert_in", False)
        invert_out: bool = args.get("invert_out", False)
        pattern = _PATTERNS.get(args.get("pattern", "circle"), 0)

        # README §3: フェードと同型の進捗ランプ。違いは「どちらのランプが勝ったか」を
        # 覚えておいて、勝った側の`反転`チェックボックスを採る点。
        g, from_out = in_out_ramp(params, wipe_in, wipe_out)
        if g >= 1.0:
            # 区間の内側。原作の `return 1` = ワーカーすら起動せず素通し。
            return None

        # README §5: 同じ1個の反転値が「時間方向(ここ)」と「空間方向(ワーカー内の
        # 比較演算子の入れ替え)」の両方に効くので、反転は単純な逆再生にはならない。
        invert = invert_out if from_out else invert_in
        if invert:
            g = 1.0 - g

        w, h = params.width, params.height

        # --- パターン生成(README §4) ---
        # 型の大きさは入力(オブジェクト)と同じなので、シェーダーは寸法を
        # inputTex[0] から取る。画素値そのものは読まない。
        mask_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
            self.mask_shader, struct.pack("iif", pattern, int(invert), g), w, h
        )

        # --- 境界のぼかし(README §6) ---
        # 半径をr_hi/r_loに分割し、それぞれ垂直→水平の2軸ペアを走らせる
        # (ぼかし=0で0パス、1で2パス、2以上で常に4パス)。キャンバスは伸ばさない
        # ので offset=0、端は生存サンプル数で割る(divisor_mode=1、README §11)。
        for radius in split_radius(blur):
            if radius <= 0:
                continue
            for step_x, step_y in ((0, 1), (1, 0)):
                mask_branch = mask_branch.add_wgsl(
                    self.box_blur_dir_shader,
                    pack_box_blur_dir_params(radius, step_x, step_y, w, h, divisor_mode=1),
                    w,
                    h,
                )

        # --- 最終合成(README §7) ---
        # 何もしないブランチが上流の状態(元のオブジェクト)をそのまま素通しする。
        original_branch = gpu_util.PyImageGenerateBuilder()
        builder = gpu_util.PyImageGenerateBuilder().add_parallel_wgsl([original_branch, mask_branch]).add_wgsl(
            self.composite_shader, None, w, h
        )

        return GeneratorBuilderReturn(builder, ItemResult(w, h))
