import math
import struct

import aperio_plugin
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.random import seed_u32
from ...common.shader_loader import effect_dirs, shared_shader


class RasterEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.raster_effect"
        self.display_name = "ラスター"
        self.description = "Shifts each row (or column) sideways along a sine wave, warping the image."

        current_dir, _ = effect_dirs(__file__)
        self.raster_shader = shared_shader("raster", current_dir, "raster.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="amplitude_px", title="横幅", default_value=100.0, suffix="px", min=0.0, max=2000.0
                ),
                RequestStructureParameter.Float(
                    id="wavelength_px", title="高さ", default_value=100.0, suffix="px", min=0.0, max=2000.0
                ),
                RequestStructureParameter.Float(
                    id="speed", title="周期", default_value=1.0, suffix="/s", min=-40.0, max=40.0
                ),
                RequestStructureParameter.Bool(
                    id="vertical", title="縦ラスター", default_value=False
                ),
                RequestStructureParameter.Bool(
                    id="random_amplitude", title="ランダム振幅", default_value=False
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | None:
        args = params.args
        amplitude_px = float(args.get("amplitude_px", 100.0))
        wavelength_px = float(args.get("wavelength_px", 100.0))
        speed = float(args.get("speed", 1.0))
        vertical = bool(args.get("vertical", False))
        random_amplitude = bool(args.get("random_amplitude", False))
        if amplitude_px == 0.0 or wavelength_px == 0.0:
            return None

        w, h = params.width, params.height
        fps = aperio_plugin.store_manager.get_state().frame_state.fps
        elapsed_seconds = (params.frame_number - params.layer.start) / fps
        dimension = w if vertical else h
        phase = elapsed_seconds * speed * wavelength_px - dimension / 2.0

        range_px = math.ceil(abs(amplitude_px))
        if not vertical:
            y0, y1 = 0, h
            x0, x1 = self._grow_1d(w, range_px)
        else:
            x0, x1 = 0, w
            y0, y1 = self._grow_1d(h, range_px)
        ow, oh = x1 - x0, y1 - y0
        shift_offset = x0 if not vertical else y0

        effect_seed = seed_u32(params.structure_id) if random_amplitude else 0
        shader_params = struct.pack(
            "fffiiIiii",
            amplitude_px, wavelength_px, phase,
            int(vertical), int(random_amplitude), effect_seed,
            shift_offset, ow, oh,
        )
        center_x = w // 2 - (x0 + ow // 2)
        center_y = h // 2 - (y0 + oh // 2)
        return GeneratorWgslReturn(
            self.raster_shader, shader_params, ItemResult(ow, oh, center_x=center_x, center_y=center_y)
        )

    def _grow_1d(self, dim: int, range_px: int) -> tuple[int, int]:
        lo, hi = -range_px, dim + range_px
        max_dim = aperio_plugin.image_generator.maximum_texture_size
        lim = min(max_dim, dim + 2 * range_px)
        trim_lo = True
        while hi - lo > lim:
            if trim_lo:
                lo += 1
            else:
                hi -= 1
            trim_lo = not trim_lo
        return lo, hi
