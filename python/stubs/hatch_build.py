"""wheel ビルド時だけ build/stub/ のスタブをパッケージに取り込む hatch のビルドフック。

editable インストール(`uv sync`)では .pth で build/stub/ を直接参照するので、スタブが未生成でも
インストールできるようにしている。配布用の wheel は `uv build --wheel python/stubs` で作る。
(sdist には build/stub/ が含まれないため、sdist 経由のビルドは対象外。)
"""

from pathlib import Path

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

STUB_DIR = Path(__file__).resolve().parent.parent.parent / "build" / "stub"
PACKAGES = ("aperio", "aperio_plugin")


class StubBuildHook(BuildHookInterface):
    def initialize(self, version: str, build_data: dict) -> None:
        if version != "standard":
            return
        for name in PACKAGES:
            src = STUB_DIR / name
            if not src.is_dir():
                raise FileNotFoundError(f"No stub found: {src}")
            build_data["force_include"][str(src)] = name
