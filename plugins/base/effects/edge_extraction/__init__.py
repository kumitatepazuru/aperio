import struct

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader

# edge_magnitude.wgsl の MODE_* と対応する。実機は`輝度エッジを抽出`/`透明度エッジを抽出`の
# 2チェックボックスだが、func_WndProc が排他制御しているので実質3択のラジオボタン
# (exedit-inspect edge_extraction README §4。両方ONは check[0] 勝ちでUIからは到達不可、
# 両方OFFが一番重い`色エッジ`)。到達可能な状態そのものをListで表現する。
_MODES = {"luma": 0, "alpha": 1, "color": 2}


class EdgeExtractionEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.edge_extraction_effect"
        self.display_name = "エッジ抽出"
        self.description = "Extracts edges with a Prewitt operator and renders them as the alpha of a single flat light color."

        current_dir, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")

        # 書き込む値は clamp 済みの符号なし[0,1]のアルファだけなので min_output_format は不要。
        self.magnitude_shader = compose_common_shader(
            "edge_extraction_magnitude", [color_module], current_dir, "edge_magnitude.wgsl"
        )
        # 出力段は`シャドー`・`縁取り`と同じ共通パス。同一ソース・同一フォーマットフロアなので
        # name も揃えてパイプラインキャッシュを共有する。
        self.encode_color_shader = shared_shader("encode_color", common_dir, "encode_color.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="strength",
                    title="強さ",
                    default_value=100.0,
                    suffix="%",
                    min=0.0,
                    max=1000.0,
                ),
                RequestStructureParameter.Float(
                    id="threshold",
                    title="しきい値",
                    default_value=0.0,
                    suffix="%",
                    min=-100.0,
                    max=100.0,
                ),
                RequestStructureParameter.List(
                    id="mode",
                    title="抽出モード",
                    values={
                        "luma": "輝度エッジを抽出",
                        "alpha": "透明度エッジを抽出",
                        "color": "色エッジを抽出",
                    },
                    default_value="luma",
                ),
                RequestStructureParameter.Color(
                    id="color",
                    title="色の設定",
                    default_value=(1.0, 1.0, 1.0, 1.0),
                    use_alpha=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn:
        args = params.args

        # edge_magnitude.wgslの `clamp((mag-threshold)*strength, 0, 1)` が最終値を
        # 自己ガードするため、ここでの範囲チェックは不要(UIの100% = 1.0に正規化するだけ)。
        strength = args.get("strength", 100.0) / 100.0
        threshold = args.get("threshold", 0.0) / 100.0
        mode = _MODES.get(args.get("mode", "luma"), _MODES["luma"])
        color = args.get("color", (1.0, 1.0, 1.0, 1.0))

        w, h = params.width, params.height

        # README §2: 早期returnが1つも無い。`強さ`=0でも全画素を走査して完全な透明画像を書く
        # (`ぼかし`/`閃光`/`拡散光`/`グロー`と違う点)。キャンバス拡張も`サイズ固定`も無い。
        mask = gpu_util.PyImageGenerateBuilder().add_wgsl(
            self.magnitude_shader, struct.pack("iff", mode, strength, threshold), w, h
        )
        builder = mask.add_wgsl(
            self.encode_color_shader, struct.pack("ffff", 1.0, color[0], color[1], color[2]), w, h
        )

        return GeneratorBuilderReturn(builder, ItemResult(w, h))
