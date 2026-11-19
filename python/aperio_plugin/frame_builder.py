from __future__ import annotations

import math
import struct

from aperio import gpu_util
from aperio.item_structures import ItemResult
from .plugin_base.generator_base import (
    GeneratorBuilderReturn,
    GeneratorFuncReturn,
    GeneratorTextureReturn,
    GeneratorShaderReturn,
)


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
    generate_result: "GeneratorShaderReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn",
) -> gpu_util.PyImageGenerateBuilder:
    """GeneratorShaderReturn/FuncReturn/TextureReturn/BuilderReturn のいずれかを builder に適用する。
    video_renderer の object/effect チェーンから使う共通ヘルパー。"""
    item_result = generate_result.item_result
    if isinstance(generate_result, GeneratorShaderReturn):
        return builder.add_shader(generate_result.compiled, generate_result.params, item_result.width, item_result.height)
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
        return builder.add_parallel([generate_result.builder])
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


def pack_layer_params(inv: list[list[float]], center_x: float, center_y: float, alpha: float) -> bytes:
    """逆ホモグラフィ・基点オフセット・透明度から、逆ホモグラフィで合成するシェーダー向けのLayerParams(64バイト)を作る。
    struct LayerParams: inv_transform (mat3x3), center_x, center_y, alpha, _pad -> 64 bytes
    """
    return struct.pack(
        "<16f",
        inv[0][0], inv[1][0], inv[2][0], 0.0,
        inv[0][1], inv[1][1], inv[2][1], 0.0,
        inv[0][2], inv[1][2], inv[2][2], 0.0,
        center_x, center_y, alpha, 0.0,
    )


IDENTITY_3X3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]


def layer_homography(result: ItemResult, canvas_height: int) -> tuple[list[list[float]], float, bool]:
    """1レイヤー分の配置を表す順方向ホモグラフィを求める。

    レイヤーを「ローカル z=0 平面上にある、基点(ピボット)中心のテクスチャ矩形」とみなし、
    拡大率 -> 3D回転 -> 平行移動(X/Y/Z) -> 透視投影 の順で画面へ送る。平面なので
    この一連の変換は3x3ホモグラフィ1枚で表せる。変形はすべて `result` の絶対値から読む。

    offscreenのときは、最終描画は矩形転送なので、X/Y の平行移動(整数画素への0方向切り捨て)と基点オフセットだけを使う。

    Returns:
        (forward, alpha, visible)。forward はレイヤーのローカル座標(基点原点のpx)から
        キャンバス中央原点のpxへの同次変換。visible が False のとき(平面が真横を向いている /
        カメラより手前にある)は画面に何も映らない。
    """
    if result.offscreen:
        tx = float(math.trunc(result.pos[0]))
        ty = float(math.trunc(result.pos[1]))
        return [[1.0, 0.0, tx], [0.0, 1.0, ty], [0.0, 0.0, 1.0]], 1.0, True

    # 整数に丸めず実数のまま持つ (実機の Q12 = 1/4096px 相当のサブピクセル座標)
    eff_x, eff_y, eff_z = result.pos

    rx_rad = math.radians(result.rotate[0])
    ry_rad = math.radians(result.rotate[1])
    rz_rad = math.radians(result.rotate[2])

    eff_scale_x, eff_scale_y = result.scale

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
    alpha = max(0.0, min(1.0, result.alpha))
    # 平面が真横を向いている(退化) / オブジェクトがカメラ位置より手前にある
    visible = d + eff_z > 0.0 and invert_3x3(forward) is not None
    return forward, alpha, visible


def layer_params(result: ItemResult, canvas_height: int) -> bytes:
    """1レイヤー分の、逆ホモグラフィで合成するシェーダー向けの `LayerParams`(64バイト)を組み立てる。
    配置の求め方は `layer_homography` を参照。シェーダーには逆行列を渡す。"""
    forward, alpha, visible = layer_homography(result, canvas_height)
    inv = invert_3x3(forward) if visible else None
    if inv is None:
        # 画面上では何も映らないので、alpha=0 でシェーダー側にスキップさせる。
        inv = IDENTITY_3X3
        alpha = 0.0

    return pack_layer_params(inv, result.center_x, result.center_y, alpha)


def blend_layer_params(result: ItemResult, canvas_height: int) -> bytes:
    """1レイヤー分の、ブレンドモードのシェーダーに渡す `BlendLayerParams`(64バイト)を組み立てる
    (並びは shaders/blend/aperio_blend.slang と同じ)。頂点シェーダーがレイヤー矩形の四隅を
    順方向ホモグラフィで画面へ送るので、逆行列ではなく forward の各行を渡す。

    struct BlendLayerParams: transform_row0..2 (float4 x3), center (float2), alpha, _pad -> 64 bytes
    """
    forward, alpha, visible = layer_homography(result, canvas_height)
    if not visible:
        # 何も描かせない(頂点が退化しても、フラグメントは alpha=0 で必ず捨てられる)
        forward = IDENTITY_3X3
        alpha = 0.0

    return struct.pack(
        "<16f",
        forward[0][0], forward[0][1], forward[0][2], 0.0,
        forward[1][0], forward[1][1], forward[1][2], 0.0,
        forward[2][0], forward[2][1], forward[2][2], 0.0,
        result.center_x, result.center_y, alpha, 0.0,
    )
