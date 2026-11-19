import os.path
from collections.abc import Iterator
from dataclasses import dataclass

from aperio import gpu_util, logger

BLEND_SHADER_DIR = os.path.join(os.path.dirname(__file__), "shaders", "blend")
"""ブレンドモード用ヘルパー(`aperio_blend.slang`)があるディレクトリ。"""

DEFAULT_BLEND_MODE_ID = "normal" # 既定のブレンドモード

FALLBACK_BLEND_MODE_DISPLAY_NAME = "通常" # 組み込みの通常合成の名前


@dataclass(frozen=True)
class BlendMode:
    """
    登録済みのブレンドモード。
    """

    id: str
    display_name: str
    shader: gpu_util.PyCompiledShader
    owner: str
    """登録したメインプラグインの名前"""


class BlendModeRegistry:
    """ブレンドモードを id で一元管理するレジストリ。登録順を保つ。"""

    def __init__(self) -> None:
        self._modes: dict[str, BlendMode] = {}

    def add(self, owner: str, id: str, display_name: str, shader: gpu_util.PyCompiledShader) -> bool:
        """
        ブレンドモードを登録する。id の重複や不正な shader はエラーをログに出してスキップする。

        Returns:
            登録できたら True
        """
        if not isinstance(id, str) or not id:
            logger.error(f"Plugin {owner}: blend mode id must be a non-empty string (got {id!r}). Skipping.")
            return False
        if id in self._modes:
            logger.error(
                f"Plugin {owner}: blend mode '{id}' is already registered by {self._modes[id].owner}. Skipping."
            )
            return False
        if not isinstance(shader, gpu_util.PyCompiledShader):
            logger.error(
                f"Plugin {owner}: blend mode '{id}' must be a PyCompiledShader "
                f"(got {type(shader).__name__}). Skipping."
            )
            return False
        if not shader.is_raster:
            logger.error(
                f"Plugin {owner}: blend mode '{id}' must be a raster shader. Skipping."
            )
            return False
        if not shader.has_sampler:
            logger.error(
                f"Plugin {owner}: blend mode '{id}' has no sampler, but aperio_blend.slang samples the layer. Skipping."
            )
            return False

        self._modes[id] = BlendMode(id, display_name, shader, owner)
        return True

    def get(self, id: str) -> BlendMode | None:
        return self._modes.get(id)

    def remove_owned_by(self, owner: str) -> None:
        """指定されたメインプラグインが登録したブレンドモードをすべて取り除く(初期化に失敗したプラグインの後始末用)。"""
        for id in [i for i, mode in self._modes.items() if mode.owner == owner]:
            del self._modes[id]

    def display_names(self) -> dict[str, str]:
        """UI 向けに、登録順の (id → display_name) を返す。"""
        return {mode.id: mode.display_name for mode in self._modes.values()}

    def __iter__(self) -> Iterator[BlendMode]:
        return iter(self._modes.values())

    def __len__(self) -> int:
        return len(self._modes)
