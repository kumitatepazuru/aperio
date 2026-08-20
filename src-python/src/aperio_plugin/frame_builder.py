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


# 合成に使う固定カメラの焦点距離 = キャンバス高さ * この値 (垂直画角 ≒ 28度)。
# 実機(`標準描画`/`拡張描画`)の投影パラメータは未解析なので、これは見た目で決めた既定値。
# 解像度に依らず同じ見え方になるよう、絶対pxではなくキャンバス高さの倍率で持つ。
CAMERA_DISTANCE_RATIO = 2.0

# ホモグラフィが退化しているとみなす行列式の閾値。X軸/Y軸回転が±90度のとき
# レイヤー平面は真横を向き、画面上では面積0の線分になる。
DEGENERATE_DET_EPSILON = 1e-9


def invert_3x3(m: list[list[float]]) -> list[list[float]] | None:
    """3x3行列を余因子行列/行列式で反転する。退化しているならNone。"""
    c00 = m[1][1] * m[2][2] - m[1][2] * m[2][1]
    c01 = m[1][2] * m[2][0] - m[1][0] * m[2][2]
    c02 = m[1][0] * m[2][1] - m[1][1] * m[2][0]
    det = m[0][0] * c00 + m[0][1] * c01 + m[0][2] * c02
    if abs(det) < DEGENERATE_DET_EPSILON:
        return None
    inv_det = 1.0 / det
    return [
        [
            c00 * inv_det,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det,
        ],
        [
            c01 * inv_det,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det,
        ],
        [
            c02 * inv_det,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det,
        ],
    ]


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
        # 空のbuilder（ステップ未追加）の場合はパイプラインを変更しない
        if not generate_result.builder.get_id_tree():
            return builder
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
    canvas_height: int,
) -> None:
    """1レイヤーぶんの `LayerParams` を組み立てて `generator_params` に積む。

    レイヤーを「ローカル z=0 平面上にある、基点(ピボット)中心のテクスチャ矩形」とみなし、
    拡大率 -> 3D回転 -> 平行移動(X/Y/Z) -> 透視投影 の順で画面へ送る。平面なので
    この一連の変換は3x3ホモグラフィ1枚で表せる。compose.wgsl には逆行列を渡す。
    """
    pos = result.pos or (0.0, 0.0, 0.0)
    # 整数に丸めず実数のまま持つ (実機の Q12 = 1/4096px 相当のサブピクセル座標)
    eff_x = item.x + pos[0]
    eff_y = item.y + pos[1]
    eff_z = pos[2]

    rotate = result.rotate or (0.0, 0.0, 0.0)
    rx_rad = math.radians(rotate[0])
    ry_rad = math.radians(rotate[1])
    rz_rad = math.radians(item.rotation + rotate[2])

    eff_center_x = result.center_x or 0.0
    eff_center_y = result.center_y or 0.0

    scale = result.scale or (1.0, 1.0)
    eff_scale_x = (item.scale / 100.0) * scale[0]
    eff_scale_y = (item.scale / 100.0) * scale[1]

    # 3D回転行列 R = Rz * Ry * Rx (3行目は透視投影で使う奥行き成分)
    cx, sx = math.cos(rx_rad), math.sin(rx_rad)
    cy, sy = math.cos(ry_rad), math.sin(ry_rad)
    cz, sz = math.cos(rz_rad), math.sin(rz_rad)

    r00 = cz * cy
    r01 = cz * sy * sx - sz * cx
    r10 = sz * cy
    r11 = sz * sy * sx + cz * cx
    r20 = -sy
    r21 = cy * sx

    # 原点から -z 側に距離 d のカメラで z=0 面へ投影する: screen = (X, Y) * d / (d + Z)。
    # Z=0 (回転もZ座標も無い) なら screen = (X, Y) で、ただのアフィン変換に退化する。
    d = CAMERA_DISTANCE_RATIO * canvas_height
    forward = [
        [r00 * eff_scale_x, r01 * eff_scale_y, eff_x],
        [r10 * eff_scale_x, r11 * eff_scale_y, eff_y],
        [r20 * eff_scale_x / d, r21 * eff_scale_y / d, (d + eff_z) / d],
    ]
    inv = invert_3x3(forward)

    alpha = (item.alpha / 100.0) * (result.alpha if result.alpha is not None else 1.0)
    alpha = max(0.0, min(1.0, alpha))

    if inv is None or d + eff_z <= 0.0:
        # 平面が真横を向いている / オブジェクトがカメラ位置より手前にある。
        # どちらも画面上では何も映らないので、alpha=0 でシェーダー側にスキップさせる。
        inv = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        alpha = 0.0

    # WGSLのmat3x3は列優先で、各列は16バイトにパディングされる。
    # struct LayerParams: inv_transform (mat3x3), center_x, center_y, alpha, _pad -> 64バイト
    params_bytes = struct.pack(
        "<16f",
        inv[0][0], inv[1][0], inv[2][0], 0.0,
        inv[0][1], inv[1][1], inv[2][1], 0.0,
        inv[0][2], inv[1][2], inv[2][2], 0.0,
        eff_center_x, eff_center_y, alpha, 0.0,
    )
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
