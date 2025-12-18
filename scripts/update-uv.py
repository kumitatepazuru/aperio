import os
import requests
import zipfile
import tarfile
import platform
import argparse
from io import BytesIO

UV_LINK = "https://github.com/astral-sh/uv/releases/latest/download"
FILES = [
    "uv-x86_64-unknown-linux-gnu.tar.gz",
    "uv-aarch64-unknown-linux-gnu.tar.gz",
    "uv-x86_64-apple-darwin.tar.gz",
    "uv-aarch64-apple-darwin.tar.gz",
    "uv-x86_64-pc-windows-msvc.zip",
    "uv-aarch64-pc-windows-msvc.zip",
]

parser = argparse.ArgumentParser()
parser.add_argument("-a", "--all", action="store_true", help="Download all files")
args = parser.parse_args()

def download_and_decompress(url, output_path, decompressed_dir):
    """
    ファイルをダウンロードして解凍する
    """
    print(f"Downloading {url}...")
    response = requests.get(url, allow_redirects=True, stream=True)
    response.raise_for_status()

    ext = ".exe" if "windows" in url else ""

    if url.endswith(".zip"):
        with zipfile.ZipFile(BytesIO(response.content)) as z:
            for member in z.infolist():
                if member.filename.endswith("uv") or member.filename.endswith("uv.exe"):
                    # uv(.exe)だけを抽出して保存
                    with open(os.path.join(decompressed_dir, f"uv{ext}"), "wb") as f:
                        f.write(z.read(member.filename))
                    break
    elif url.endswith(".tar.gz"):
        with tarfile.open(fileobj=BytesIO(response.content), mode="r:gz") as tar:
            for member in tar.getmembers():
                if member.name.endswith("uv"):
                    # uvだけを抽出して保存
                    extracted_file = tar.extractfile(member)
                    if extracted_file:
                        with open(os.path.join(decompressed_dir, "uv"), "wb") as f:
                            f.write(extracted_file.read())
                        break

    # URLからOS/archを判定
    url_name = os.path.basename(url).split(".")[0]  # 拡張子なしのファイル名
    if "windows" in url_name:
        os_name = "win"
    elif "apple-darwin" in url_name:
        os_name = "mac"
    elif "linux" in url_name:
        os_name = "linux"
    else:
        os_name = "unknown"

    if "x86_64" in url_name:
        arch = "x64"
    elif "aarch64" in url_name:
        arch = "arm64"
    else:
        arch = "unknown"

    # resources/[os]-[arch]形式のパスを作成
    platform_output_path = os.path.join(output_path, f"{os_name}-{arch}", "bin")
    os.makedirs(platform_output_path, exist_ok=True)

    final_output_path = os.path.join(platform_output_path, f"uv{ext}")
    os.rename(os.path.join(decompressed_dir, f"uv{ext}"), final_output_path)
    # 実行権限を付与
    if not ext:  # Windows以外
        os.chmod(final_output_path, 0o755)
    print(f"Downloaded and decompressed to {final_output_path}")

def main():
    """
    メイン処理
    """
    output_path = os.path.join("resources")
    os.makedirs(output_path, exist_ok=True)

    decompressed_dir = os.path.join(output_path, "tmp")
    if not os.path.exists(decompressed_dir):
        os.makedirs(decompressed_dir, exist_ok=True)

    try:
        for file in FILES:
            if not platform.system().lower() in file and not args.all:
                print(f"Skipping {file} for current platform.")
                continue

            file_url = f"{UV_LINK}/{file}"
            download_and_decompress(file_url, output_path, decompressed_dir)

        print("All files downloaded and decompressed successfully.")
    except Exception as e:
        print(f"Error: {e}")
    finally:
        # 一時ディレクトリを削除
        if os.path.exists(decompressed_dir):
            import shutil
            shutil.rmtree(decompressed_dir)

if __name__ == "__main__":
    main()