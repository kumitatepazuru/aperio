from . import AperioManager as AperioManager
from .plugin_base.generator_base import GeneratorBuilderReturn as GeneratorBuilderReturn, GeneratorFuncReturn as GeneratorFuncReturn, GeneratorTextureReturn as GeneratorTextureReturn, GeneratorWgslReturn as GeneratorWgslReturn
from aperio import gpu_util as gpu_util
from aperio.item_structures import AdditionalItem as AdditionalItem, ItemResult as ItemResult, ItemStructure

CAMERA_DISTANCE_RATIO: float
DEGENERATE_DET_EPSILON: float

def invert_3x3(m: list[list[float]]) -> list[list[float]] | None:
    """3x3行列を余因子行列/行列式で反転する。退化しているならNone。"""
def apply_generate_result(builder: gpu_util.PyImageGenerateBuilder, generate_result: GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn) -> gpu_util.PyImageGenerateBuilder:
    """GeneratorWgslReturn/FuncReturn/TextureReturn/BuilderReturn のいずれかを builder に適用する。
    _process_video_item のオブジェクト生成・エフェクトチェーンの両方から使う共通ヘルパー。"""
def last_leaf_id(id_tree_list: list) -> str:
    """get_id_tree()が返す、全ステップのidを並べたリストから、最後に追加された
    ステップのleaf idを取り出す。末尾要素を見て、それが`dict`(Parallelステップ)なら
    その唯一の値(各ブランチの全ステップidリストからなるリスト)の[-1](=最後のブランチ、
    これも1つのリスト)の[-1]を、というように末尾を再帰的にたどる。"""
def collect_additional_item(item_result: ItemResult, behind_items: list[AdditionalItem], ahead_items: list[AdditionalItem]) -> None: ...
def append_frame_entry(item_builder: gpu_util.PyImageGenerateBuilder, item: ItemStructure.Video, result: ItemResult, layer_builders: list[gpu_util.PyImageGenerateBuilder], generator_params: list[bytes], canvas_height: int) -> None:
    """1レイヤーぶんの `LayerParams` を組み立てて `generator_params` に積む。

    レイヤーを「ローカル z=0 平面上にある、基点(ピボット)中心のテクスチャ矩形」とみなし、
    拡大率 -> 3D回転 -> 平行移動(X/Y/Z) -> 透視投影 の順で画面へ送る。平面なので
    この一連の変換は3x3ホモグラフィ1枚で表せる。compose.wgsl には逆行列を渡す。
    """
def resolve_additional_entry(manager: AperioManager, additional: AdditionalItem, layer_id: str, frame_number: int, width: int, height: int, structure_id_map: dict[str, tuple[str, int, int]]) -> tuple[gpu_util.PyImageGenerateBuilder, 'ItemStructure.Video', ItemResult] | None: ...
