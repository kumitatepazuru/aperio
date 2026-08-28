import os

import aperio_plugin
from aperio.avloader import PyVideoLoader
from aperio.gpu_util import PyCompiledTextureFunc


class VideoLoaderCache:
    """呼び出し元(オブジェクト/エフェクト)の id をキーにVideoLoaderを作って使い回すキャッシュ"""

    def __init__(self, func_id: str) -> None:
        self._func_id = func_id
        self._entries: dict[str, tuple[str, PyVideoLoader, PyCompiledTextureFunc]] = {}

    def ensure_loaded(self, id: str, path: str) -> None:
        if not id or not path:
            return
        existing = self._entries.get(id)
        if existing is not None and existing[0] == path:
            return
        if not os.path.exists(path):
            return
        loader = PyVideoLoader(path=path, image_generator=aperio_plugin.image_generator)
        func = PyCompiledTextureFunc(self._func_id, loader.get_frame_for_pipeline)
        self._entries[id] = (path, loader, func)

    def get(self, id: str) -> tuple[PyVideoLoader, PyCompiledTextureFunc] | None:
        entry = self._entries.get(id)
        if entry is None:
            return None
        _, loader, func = entry
        return loader, func

    def get_or_load(self, id: str, path: str) -> tuple[PyVideoLoader, PyCompiledTextureFunc] | None:
        """`ensure_loaded` + `get` をまとめたヘルパー"""
        self.ensure_loaded(id, path)
        return self.get(id)
