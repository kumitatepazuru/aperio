import struct

from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.params import make_generator_information
from ..common.shader_loader import compose_common_shader, effect_dirs, lib_module

# exedit-inspect luma_key README §2: ex_data.typeの4項目。func_procの分岐は
# 0/1/2/elseなので、コンボ以外の値が来た場合はelse(=2)側の式に倒れる。
_KEY_TYPES = {
    "dark": 0,
    "light": 1,
    "band": 2,
    "band_hard": 3,
}


class LumaKeyEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.luma_key_effect"
        self.display_name = "ルミナンスキー"
        self.description = "Keys out pixels within a luminance band around a base level, with optional linear edge falloff."

        current_dir, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")

        self.luma_key_shader = compose_common_shader("luma_key", [color_module], current_dir, "luma_key.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="base_luminance",
                    title="基準輝度",
                    default_value=2048,
                    min=-4096,
                    max=8192,
                ),
                RequestStructureParameter.Int(
                    id="blur",
                    title="ぼかし",
                    default_value=512,
                    min=0,
                    max=4096,
                ),
                RequestStructureParameter.List(
                    id="key_type",
                    title="透過方法",
                    values={
                        "dark": "暗い部分を透過",
                        "light": "明るい部分を透過",
                        "band": "明暗部分を透過",
                        "band_hard": "明暗部分を透過(ぼかし無し)",
                    },
                    default_value="dark",
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn:
        args = params.args
        # README §7の通りこのエフェクトは元々クランプを持たない。luma_key.wgslの
        # 分岐(t<0とt<blurの排他性)によりblur<=0でもゼロ除算は起きないため安全。
        base_ui = args.get("base_luminance", 2048)
        blur_ui = args.get("blur", 512)
        key_type = _KEY_TYPES.get(args.get("key_type", "dark"), 0)

        # README §3: 表示スケールが両方とも1で `/1000` のマジックナンバー除算も
        # 無いので、生値がそのまま PIXEL_YC.y (0..4096) と同じ単位。他エフェクトの
        # 正規化空間(bt601_luma の 0..1)に合わせるため /4096.0 する。
        base = base_ui / 4096.0
        blur = blur_ui / 4096.0

        shader_params = struct.pack("ffi", base, blur, key_type)

        return GeneratorWgslReturn(self.luma_key_shader, shader_params, ItemResult(params.width, params.height))
