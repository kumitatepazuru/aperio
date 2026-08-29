import functools
import struct

from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from .noise_field import build_noise_field
from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module

_TYPE_INDEX = {"type1": 0, "type2": 1, "type3": 2, "type4": 3, "type5": 4, "type6": 5}
_MODE_INDEX = {"alpha": 0, "luminance": 1}
_FIELD_LEN = 65536


@functools.lru_cache(maxsize=1)
def _packed_noise_field() -> bytes:
    """65536個のf32をshader_paramsの末尾に連結する用。乱数場は不変なので
    バイト列化した結果もプロセス内で1回だけ計算してキャッシュする。"""
    return struct.pack(f"{_FIELD_LEN}f", *build_noise_field())


class NoiseEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.noise_effect"
        self.display_name = "ノイズ"
        self.description = "Multiplies animated 3D value noise into the alpha or luminance channel."

        current_dir, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")
        self.noise_shader = compose_common_shader("noise", [color_module], current_dir, "noise.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="strength", title="強さ", default_value=100.0, suffix="%", min=0.0, max=200.0
                ),
                RequestStructureParameter.Float(
                    id="velocity_x", title="速度X", default_value=0.0, suffix="cell/f", min=-100.0, max=100.0
                ),
                RequestStructureParameter.Float(
                    id="velocity_y", title="速度Y", default_value=0.0, suffix="cell/f", min=-100.0, max=100.0
                ),
                RequestStructureParameter.Float(
                    id="change_speed", title="変化速度", default_value=0.0, suffix="/f", min=-100.0, max=100.0
                ),
                RequestStructureParameter.Float(
                    id="period_x", title="周期X", default_value=1.0, min=0.0, max=100.0
                ),
                RequestStructureParameter.Float(
                    id="period_y", title="周期Y", default_value=1.0, min=0.0, max=100.0
                ),
                RequestStructureParameter.Float(
                    id="threshold", title="しきい値", default_value=0.0, suffix="%", min=0.0, max=100.0
                ),
                RequestStructureParameter.List(
                    id="mode",
                    title="適用先",
                    values={"alpha": "アルファ値と乗算", "luminance": "輝度と乗算"},
                    default_value="alpha",
                ),
                RequestStructureParameter.List(
                    id="noise_type",
                    title="Type",
                    values={
                        "type1": "Type1", "type2": "Type2", "type3": "Type3",
                        "type4": "Type4", "type5": "Type5", "type6": "Type6",
                    },
                    default_value="type1",
                ),
                RequestStructureParameter.Int(
                    id="seed", title="シード", default_value=0, min=0, max=65535
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn:
        args = params.args
        strength = max(0.0, min(200.0, float(args.get("strength", 100.0)))) / 100.0
        velocity_x = float(args.get("velocity_x", 0.0))
        velocity_y = float(args.get("velocity_y", 0.0))
        change_speed = float(args.get("change_speed", 0.0))
        # exedit-inspect noise README §3: `周期`は生値0~100の密度倍率であって
        # 「セルの大きさ」ではない ―― 大きいほど1画素あたりに詰まるセル数が
        # 増える(=模様が細かく/小さくなる)。0で「1画素も進まない」平坦な場になる。
        period_x = max(0.0, min(100.0, float(args.get("period_x", 1.0))))
        period_y = max(0.0, min(100.0, float(args.get("period_y", 1.0))))
        threshold = max(0.0, min(100.0, float(args.get("threshold", 0.0)))) / 100.0
        mode = _MODE_INDEX.get(args.get("mode", "alpha"), 0)
        noise_type = _TYPE_INDEX.get(args.get("noise_type", "type1"), 0)
        seed = int(args.get("seed", 0))

        # exedit-inspect noise README §9: `速度X`/`速度Y`/`変化速度`の時間積分は経過フレーム数に対して行われる
        elapsed_frames = params.frame_number - params.layer.start
        ox = elapsed_frames * velocity_x
        oy = elapsed_frames * velocity_y
        oz = elapsed_frames * change_speed

        # exedit-inspect noise README §4/§9: ノイズ場をオブジェクトの中心に
        # 合わせる(原点は既定でオブジェクトの左上・セル単位のため、幅・高さの
        # 半分をセル単位に換算して引く。`周期`は密度倍率なので、画素数×周期が
        # セル数になる)。
        ox -= (params.width * period_x) / 2.0
        oy -= (params.height * period_y) / 2.0

        # exedit-inspect noise README §4: `シード`は乱数場を作り直すのではなく、
        # 同じ乱数場の値をox/oy/ozのオフセット源として使う(0なら何も足さない
        # = 全インスタンスが完全に同一の模様になるのがオリジナルの仕様)。
        if seed != 0:
            field = build_noise_field()
            ox += field[seed % _FIELD_LEN]
            oy += field[(seed + 1) % _FIELD_LEN]
            oz += field[(seed + 2) % _FIELD_LEN]

        shader_params = struct.pack(
            "ii",
            noise_type, mode,
        ) + struct.pack(
            "ffffff",
            period_x, period_y, ox, oy, oz, strength,
        ) + struct.pack(
            "f",
            threshold,
        ) + struct.pack(
            "ii",
            params.width, params.height,
        ) + _packed_noise_field()

        return GeneratorWgslReturn(self.noise_shader, shader_params, ItemResult(params.width, params.height))
