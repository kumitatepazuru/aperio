"""同梱用の uv バイナリを GitHub Releases から取得し dist/resources/<os>-<arch>/bin/ に配置する。

    uv run tools/update_uv.py        # 実行中のプラットフォームのみ
    uv run tools/update_uv.py -a     # 全プラットフォーム
"""

from __future__ import annotations

import argparse
import os
import platform
import tarfile
import tempfile
import zipfile
from io import BytesIO
from pathlib import Path

import requests

DIST_RESOURCES = Path(__file__).resolve().parent.parent / "dist" / "resources"

UV_LINK = "https://github.com/astral-sh/uv/releases/latest/download"
FILES = [
    "uv-x86_64-unknown-linux-gnu.tar.gz",
    "uv-aarch64-unknown-linux-gnu.tar.gz",
    "uv-x86_64-apple-darwin.tar.gz",
    "uv-aarch64-apple-darwin.tar.gz",
    "uv-x86_64-pc-windows-msvc.zip",
    "uv-aarch64-pc-windows-msvc.zip",
]


def platform_dir(file_name: str) -> str:
    """ファイル名から resources 配下のディレクトリ名 (<os>-<arch>) を決める。"""
    if "windows" in file_name:
        os_name = "win"
    elif "apple-darwin" in file_name:
        os_name = "mac"
    elif "linux" in file_name:
        os_name = "linux"
    else:
        os_name = "unknown"

    if "x86_64" in file_name:
        arch = "x64"
    elif "aarch64" in file_name:
        arch = "arm64"
    else:
        arch = "unknown"
    return f"{os_name}-{arch}"


def download_and_decompress(file_name: str, output_root: Path, tmp_dir: Path) -> None:
    """アーカイブをダウンロードし、uv(.exe) だけを取り出して配置する。"""
    url = f"{UV_LINK}/{file_name}"
    print(f"Downloading {url}...")
    response = requests.get(url, allow_redirects=True, stream=True)
    response.raise_for_status()

    ext = ".exe" if "windows" in file_name else ""
    extracted = tmp_dir / f"uv{ext}"

    if file_name.endswith(".zip"):
        with zipfile.ZipFile(BytesIO(response.content)) as z:
            for member in z.infolist():
                if member.filename.endswith(("uv", "uv.exe")):
                    extracted.write_bytes(z.read(member.filename))
                    break
    elif file_name.endswith(".tar.gz"):
        with tarfile.open(fileobj=BytesIO(response.content), mode="r:gz") as tar:
            for member in tar.getmembers():
                if member.name.endswith("uv"):
                    f = tar.extractfile(member)
                    if f:
                        extracted.write_bytes(f.read())
                        break

    if not extracted.exists():
        raise RuntimeError(f"{file_name} から uv を取り出せませんでした")

    bin_dir = output_root / platform_dir(file_name) / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    final_path = bin_dir / f"uv{ext}"
    final_path.unlink(missing_ok=True)
    extracted.replace(final_path)
    if not ext:  # Windows 以外は実行権限を付与
        os.chmod(final_path, 0o755)
    print(f"Downloaded and decompressed to {final_path}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("-a", "--all", action="store_true", help="全プラットフォーム分をダウンロードする")
    args = parser.parse_args()

    DIST_RESOURCES.mkdir(parents=True, exist_ok=True)
    system = platform.system().lower()
    with tempfile.TemporaryDirectory() as tmp:
        for file_name in FILES:
            if system not in file_name and not args.all:
                print(f"Skipping {file_name} for current platform.")
                continue
            download_and_decompress(file_name, DIST_RESOURCES, Path(tmp))
    print("All files downloaded and decompressed successfully.")


if __name__ == "__main__":
    main()
