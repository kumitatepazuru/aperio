import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

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

        current_dir = os.path.dirname(__file__)
        common_dir = os.path.join(current_dir, "..", "common")
        color_module = gpu_util.create_composable_module(os.path.join(common_dir, "lib", "color.wgsl"))

        self.luma_key_shader = PyCompiledWgsl.compose_new(
            "luma_key",
            [color_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "luma_key.wgsl")),
            aperio_plugin.image_generator,
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
        base_ui = max(-4096, min(8192, args.get("base_luminance", 2048)))
        blur_ui = max(0, min(4096, args.get("blur", 512)))
        key_type = _KEY_TYPES.get(args.get("key_type", "dark"), 0)

        # README §3: 表示スケールが両方とも1で `/1000` のマジックナンバー除算も
        # 無いので、生値がそのまま PIXEL_YC.y (0..4096) と同じ単位。他エフェクトの
        # 正規化空間(bt601_luma の 0..1)に合わせるため /4096.0 する。
        base = base_ui / 4096.0
        blur = blur_ui / 4096.0

        shader_params = struct.pack("ffi", base, blur, key_type)

        return GeneratorWgslReturn(self.luma_key_shader, shader_params, ItemResult(params.width, params.height))
