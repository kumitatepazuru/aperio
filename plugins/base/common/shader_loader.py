import os

import aperio_plugin
from aperio import gpu_util
from aperio.gpu_util import PyCompiledSlang


COMMON_DIR = os.path.dirname(os.path.abspath(__file__))
LIB_DIR = os.path.join(COMMON_DIR, "lib")


def effect_dirs(file: str) -> tuple[str, str]:
    """呼び出し元エフェクトの __file__ から (current_dir, common_dir) を返す。"""
    current_dir = os.path.dirname(file)
    return current_dir, COMMON_DIR


def load_text(path: str) -> str:
    with open(path, "r") as f:
        return f.read()


def shared_slang_shader(
    name: str,
    directory: str,
    filename: str,
    search_dirs: list[str] | None = None,
    defines: dict[str, str] | None = None,
    min_output_format: "gpu_util.WrappedImagePixelFormat | None" = None,
    input_texture_layout: str = "fixed",
    sampler_options: "gpu_util.PySamplerOptions | None" = None,
) -> PyCompiledSlang:
    """{directory}/{filename} を PyCompiledSlang として読み込む
    (各エフェクトが個別に持っていた `load()` + PyCompiledWgsl(...) の定型を
    まとめたもの)。Slangは`import`をモジュール検索パスから解決するため、
    naga_oil時代の`shared_shader`/`compose_common_shader`の使い分けは不要になり、
    このヘルパー1つで単一ファイル・複数ファイル合成のどちらにも対応する。

    `search_dirs`省略時は[directory, COMMON_DIR, LIB_DIR]
    (エフェクト固有のcommon.slang・common/直下・common/lib/のいずれもimportできる)。

    `input_texture_layout`はシェーダーの入力テクスチャ配列(group0)が固定長("fixed",
    既定)か可変長("variable")かを指定する。シェーダー自体の性質で決まる値なので、
    同じ`.slang`ファイルを使うすべての呼び出し元で同じ値を渡すこと。
    """
    resolved_search_dirs = search_dirs if search_dirs is not None else [directory, COMMON_DIR, LIB_DIR]
    return PyCompiledSlang(
        name,
        load_text(os.path.join(directory, filename)),
        aperio_plugin.image_generator,
        search_paths=resolved_search_dirs,
        defines=defines,
        min_output_format=min_output_format,
        input_texture_layout=input_texture_layout,
        sampler_options=sampler_options,
    )
