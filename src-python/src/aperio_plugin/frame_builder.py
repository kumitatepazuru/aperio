from __future__ import annotations

import math
import struct
from typing import TYPE_CHECKING

from aperio import gpu_util, logger
from aperio.item_structures import AdditionalItem, ItemResult, ItemStructure
from .plugin_base.generator_base import (
    GeneratorBuilderReturn,
    GeneratorFuncReturn,
    GeneratorTextureReturn,
    GeneratorWgslReturn,
)

if TYPE_CHECKING:
    from . import AperioManager


def apply_generate_result(
    builder: gpu_util.PyImageGenerateBuilder,
    generate_result: "GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn",
) -> gpu_util.PyImageGenerateBuilder:
    """GeneratorWgslReturn/FuncReturn/TextureReturn/BuilderReturn のいずれかを builder に適用する。
    _process_video_item のオブジェクト生成・エフェクトチェーンの両方から使う共通ヘルパー。"""
    item_result = generate_result.item_result
    if isinstance(generate_result, GeneratorWgslReturn):
        return builder.add_wgsl(generate_result.compiled, generate_result.params, item_result.width, item_result.height)
    elif isinstance(generate_result, GeneratorFuncReturn):
        return builder.add_func(generate_result.compiled, generate_result.params, item_result.width, item_result.height)
    elif isinstance(generate_result, GeneratorTextureReturn):
        return builder.add_texture_func(
            generate_result.compiled, generate_result.params, item_result.width, item_result.height
        )
    elif isinstance(generate_result, GeneratorBuilderReturn):
        # 旧add_builder(chain廃止)の代替。要素1つのparallelはchainと実行結果・id意味論が等価。
        return builder.add_parallel_wgsl([generate_result.builder])
    return builder


def last_leaf_id(id_tree_list: list) -> str:
    """get_id_tree()が返す、全ステップのidを並べたリストから、最後に追加された
    ステップのleaf idを取り出す。末尾要素を見て、それが`dict`(Parallelステップ)なら
    その唯一の値(各ブランチの全ステップidリストからなるリスト)の[-1](=最後のブランチ、
    これも1つのリスト)の[-1]を、というように末尾を再帰的にたどる。"""
    if not id_tree_list:
        raise ValueError("Cannot resolve the last id of an empty id tree list")
    node = id_tree_list[-1]
    while isinstance(node, dict):
        ((_, branches),) = node.items()  # branches: list[list[...]] (各ブランチの全ステップidリスト)
        if not branches or not branches[-1]:
            raise ValueError("Encountered an empty parallel branch while resolving a pipeline id")
        node = branches[-1][-1]
    if not isinstance(node, str):
        raise TypeError(f"Unexpected id tree leaf type: {type(node)!r}")
    return node


def collect_additional_item(
    item_result: ItemResult, behind_items: list[AdditionalItem], ahead_items: list[AdditionalItem]
) -> None:
    additional = item_result.additional_item
    if additional is not None:
        (behind_items if additional.behind else ahead_items).append(additional)


def append_frame_entry(
    item_builder: gpu_util.PyImageGenerateBuilder,
    item: "ItemStructure.Video",
    result: ItemResult,
    layer_builders: list[gpu_util.PyImageGenerateBuilder],
    generator_params: list[bytes],
) -> None:
    eff_x = item.x + (result.x or 0)
    eff_y = item.y + (result.y or 0)
    eff_rotation = item.rotation + (result.rotate or 0.0)
    eff_center_x = result.center_x or 0
    eff_center_y = result.center_y or 0

    # 回転をラジアンに変換してから回転行列を計算
    rotation_rad = math.radians(eff_rotation)
    cos_theta = math.cos(rotation_rad)
    sin_theta = math.sin(rotation_rad)
    alpha = item.alpha / 100  # 0-100 -> 0-1
    rotation_matrix = [cos_theta, sin_theta, -sin_theta, cos_theta]

    fmt = "<iiff"  # x, y, scale, alpha
    fmt += "4f"    # rotation_matrix (2x2 floats)
    fmt += "ii"    # center_x, center_y
    params_bytes = struct.pack(fmt, eff_x, eff_y, item.scale / 100, alpha, *rotation_matrix, eff_center_x, eff_center_y)
    layer_builders.append(item_builder)
    generator_params.append(params_bytes)


def resolve_additional_entry(
    manager: "AperioManager",
    additional: AdditionalItem,
    layer_id: str,
    frame_number: int,
    width: int,
    height: int,
    structure_id_map: dict[str, tuple[str, int, int]],
) -> tuple[gpu_util.PyImageGenerateBuilder, "ItemStructure.Video", ItemResult] | None:
    if not isinstance(additional.item, ItemStructure.Video):
        logger.warning(f"additional_item of layer {layer_id} is not a video item. Skipping.")
        return None
    processed = manager._process_video_item(additional.item, frame_number, width, height, structure_id_map)
    if processed is None:
        return None
    return processed[0], additional.item, processed[1]
