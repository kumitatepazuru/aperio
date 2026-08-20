import os

import aperio_plugin
from aperio.avloader import PyImageLoader
from aperio.gpu_util import PyCompiledTextureFunc

# `パターン画像ファイル`ボタン(shadow・border 共通)が受け付ける拡張子。
PATTERN_EXTENSIONS = ["png", "jpg", "jpeg", "bmp", "webp", "gif", "tiff", "tga"]


class PatternImageCache:
    """`パターン画像ファイル`パラメータのパスから PyImageLoader / PyCompiledTextureFunc を
    作って使い回すキャッシュ(shadow・border が共通で持つ定型処理をまとめたもの)。"""

    def __init__(self, func_id: str) -> None:
        self._func_id = func_id
        self._loaders: dict[str, PyImageLoader] = {}
        self._funcs: dict[str, PyCompiledTextureFunc] = {}

    def ensure_loaded(self, path: str) -> None:
        if path and path not in self._loaders and os.path.exists(path):
            loader = PyImageLoader(path=path, image_generator=aperio_plugin.image_generator)
            self._loaders[path] = loader
            self._funcs[path] = PyCompiledTextureFunc(self._func_id, loader.get_frame_for_pipeline)

    def get(self, path: str) -> tuple[PyImageLoader, PyCompiledTextureFunc] | None:
        loader = self._loaders.get(path)
        func = self._funcs.get(path)
        if loader is None or func is None:
            return None
        return loader, func
