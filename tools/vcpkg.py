"""vcpkg の依存関係(native/vcpkg_installed)に関する処理。

    uv run tools/vcpkg.py install     # 未インストールなら `vcpkg install --triplet <triplet>` を実行
    uv run tools/vcpkg.py copy-libs   # slang / dxcompiler の共有ライブラリを dist/app/ にコピー

moon の napi:vcpkg / napi:copy-libs タスクから呼ばれる。
"""

from __future__ import annotations

import argparse
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NATIVE_DIR = ROOT / "native"
DIST_APP = ROOT / "dist" / "app"

# gpu 側の C++ shim が実行時に必要とする共有ライブラリ(vcpkg の shader-slang / directx-dxc)。
_RUNTIME_LIB_PREFIXES = ("slang", "libslang", "dxcompiler", "libdxcompiler")


def vcpkg_triplet() -> str:
    """crates/*/build.rs と同じ規則で vcpkg の triplet を決める。"""
    env = os.environ.get("VCPKG_DEFAULT_TRIPLET")
    if env:
        return env
    machine = platform.machine().lower()
    arm = machine in ("arm64", "aarch64")
    system = platform.system().lower()
    if system == "windows":
        return "arm64-windows-static-md" if arm else "x64-windows-static-md"
    if system == "linux":
        return "arm64-linux" if arm else "x64-linux"
    return "aarch64-osx"


def _installed_dir() -> Path:
    return NATIVE_DIR / "vcpkg_installed" / vcpkg_triplet()


def _is_runtime_lib(name: str) -> bool:
    if not name.startswith(_RUNTIME_LIB_PREFIXES):
        return False
    # .dll (Windows) / .so, .so.N (Linux) / .dylib (macOS)。.lib などのリンク用ファイルは除く
    return name.endswith((".dll", ".dylib")) or ".so" in name


def install() -> None:
    installed = _installed_dir()
    if installed.is_dir():
        print(f"vcpkg: {installed.relative_to(ROOT)} は既にあります。スキップします")
        return

    triplet = vcpkg_triplet()
    vcpkg = shutil.which("vcpkg")
    if vcpkg is None:
        sys.exit(
            f"{installed.relative_to(ROOT)} がありませんが、vcpkg が見つかりません。"
            "docs/build.md の手順に従って vcpkg をインストールしてください。"
        )
    cmd = [vcpkg, "install", "--triplet", triplet]
    print(f"$ {' '.join(cmd)}  (cwd: {NATIVE_DIR})", flush=True)
    result = subprocess.run(cmd, cwd=NATIVE_DIR)
    if result.returncode != 0:
        sys.exit(result.returncode)


def copy_libs() -> None:
    """vcpkg_installed 内の slang / dxcompiler の共有ライブラリを .node と同じ場所に置く。"""
    installed = _installed_dir()
    DIST_APP.mkdir(parents=True, exist_ok=True)
    # Windows の DLL は bin/、Linux / macOS の共有ライブラリは lib/ にある
    for sub in ("bin", "lib"):
        src_dir = installed / sub
        if not src_dir.is_dir():
            continue
        for path in sorted(src_dir.iterdir()):
            if _is_runtime_lib(path.name):
                shutil.copy2(path, DIST_APP / path.name)
                print(f"copied {path.name} -> {DIST_APP}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["install", "copy-libs"])
    args = parser.parse_args()

    if args.command == "install":
        install()
    else:
        copy_libs()


if __name__ == "__main__":
    main()
