"""Python の共有ライブラリ(libpython / pythonXY.dll)を dist/app/ にコピーする。

pyo3 で Python を埋め込んだ .node の実行時に必要になる。`uv run` で実行すると、
プロジェクトの .venv が使っているものと同じ Python のライブラリをコピーする。
別の Python を使う場合は --python で実行ファイルを指定する。
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

DIST_APP = Path(__file__).resolve().parent.parent / "dist" / "app"

# 対象 Python から必要な情報だけ取得するための一行スクリプト
_PROBE = (
    "import json, sys, sysconfig;"
    "print(json.dumps({'major': sys.version_info.major, 'minor': sys.version_info.minor,"
    "'libdir': sysconfig.get_config_var('LIBDIR'), 'base_prefix': sys.base_prefix,"
    "'executable': sys.executable}))"
)


def _probe(python: str) -> dict:
    out = subprocess.run([python, "-c", _PROBE], capture_output=True, text=True, check=True)
    return json.loads(out.stdout)


def _find_windows(info: dict) -> list[Path]:
    major, minor = info["major"], info["minor"]
    dll_name = f"python{major}{minor}.dll"
    exe_dir = Path(info["executable"]).parent
    base = Path(info["base_prefix"])
    candidates = [exe_dir, exe_dir.parent, exe_dir / "libs", exe_dir.parent / "libs", base, base / "libs"]
    for d in candidates:
        if (d / dll_name).exists():
            found = [d / dll_name]
            # python3.dll も同じディレクトリにあればコピーする
            if (d / "python3.dll").exists():
                found.append(d / "python3.dll")
            return found
    searched = "\n".join(f"  {d}" for d in candidates)
    sys.exit(f"{dll_name} が見つかりません。探索先:\n{searched}")


def _find_unix(info: dict) -> list[Path]:
    major, minor = info["major"], info["minor"]
    libdir = info["libdir"]
    if not libdir:
        sys.exit("sysconfig から LIBDIR を取得できませんでした。")
    suffix = "dylib" if sys.platform == "darwin" else "so"
    lib = Path(libdir) / (
        f"libpython{major}.{minor}.{suffix}"
    )
    # シンボリックリンクの可能性があるので実体まで辿る
    return [lib.resolve()]


def copy_python(python: str | None = None, dest: Path = DIST_APP) -> None:
    info = _probe(python or sys.executable)
    print(f"Python version: {info['major']}.{info['minor']} ({info['executable']})")
    sources = _find_windows(info) if sys.platform == "win32" else _find_unix(info)

    dest.mkdir(parents=True, exist_ok=True)
    for src in sources:
        if not src.exists():
            sys.exit(f"共有ライブラリが見つかりません: {src}")
        # Linux は libpython3.14.so.1.0 のようなバージョン付きの実体が SONAME になるため、
        # 元のリンク名ではなく実体のファイル名でコピーする。
        shutil.copy2(src, dest / src.name)
        print(f"copied {src} -> {dest / src.name}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", help="対象の Python 実行ファイル(省略時は現在のインタプリタ)")
    args = parser.parse_args()
    copy_python(args.python)


if __name__ == "__main__":
    main()
