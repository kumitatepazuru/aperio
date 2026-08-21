import math

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.random import rand_unit


class VibrationEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.vibration_effect"
        self.display_name = "振動"
        self.description = "Shakes the object's position over time using a randomized sine wave."

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="amplitude_x", title="X", default_value=10.0, suffix="px", min=-500.0, max=500.0
                ),
                RequestStructureParameter.Float(
                    id="amplitude_y", title="Y", default_value=10.0, suffix="px", min=-500.0, max=500.0
                ),
                RequestStructureParameter.Float(
                    id="amplitude_z", title="Z", default_value=0.0, suffix="px", min=-500.0, max=500.0
                ),
                RequestStructureParameter.Int(
                    id="half_period", title="周期", default_value=1, min=1, max=100, suffix="frame"
                ),
                RequestStructureParameter.Bool(
                    id="random_strength", title="ランダムに強さを変える", default_value=True
                ),
                RequestStructureParameter.Bool(
                    id="complex_mode", title="複雑に振動", default_value=False
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn:
        args = params.args
        amp = (
            float(args.get("amplitude_x", 10.0)),
            float(args.get("amplitude_y", 10.0)),
            float(args.get("amplitude_z", 0.0)),
        )
        half_period = max(1, int(args.get("half_period", 1)))
        random_strength = bool(args.get("random_strength", True))
        complex_mode = bool(args.get("complex_mode", False))
        t = params.frame_number - params.layer.start

        if not complex_mode:
            octaves = [half_period]
        else:
            octaves = []
            k = 0
            while True:
                hp_k = half_period // (2**k)
                if hp_k <= 0:
                    break
                octaves.append(hp_k)
                k += 1

        wave_fn = math.cos if random_strength else math.sin

        disp = [0.0, 0.0, 0.0]
        for k, hp_k in enumerate(octaves):
            weight = 1.0 / (2**k)
            phase = math.pi * t / hp_k
            s = wave_fn(phase)
            if s == 0.0:
                continue
            for axis in range(3):
                if amp[axis] == 0.0:
                    continue
                if random_strength:
                    wave_index = t // hp_k
                    r = rand_unit(params.structure_id, k, axis, int(wave_index))
                else:
                    r = 1.0
                disp[axis] += weight * amp[axis] * r * s

        return GeneratorBuilderReturn(
            gpu_util.PyImageGenerateBuilder(),
            ItemResult(params.width, params.height, pos=(disp[0], disp[1], disp[2])),
        )
