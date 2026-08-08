import os

import aperio_plugin
from aperio import gpu_util
from aperio.gpu_util import PyCompiledWgsl


def effect_dirs(file: str) -> tuple[str, str]:
    """呼び出し元エフェクトの __file__ から (current_dir, common_dir) を返す。"""
    current_dir = os.path.dirname(file)
    return current_dir, os.path.join(current_dir, "..", "common")


def load_text(path: str) -> str:
    with open(path, "r") as f:
        return f.read()


def lib_module(common_dir: str, name: str):
    """common/lib/{name}.wgsl を composable module として読み込む。"""
    return gpu_util.create_composable_module(os.path.join(common_dir, "lib", f"{name}.wgsl"))


def shared_shader(
    name: str, directory: str, filename: str, output_format: "gpu_util.WrappedImagePixelFormat | None" = None
) -> PyCompiledWgsl:
    """{directory}/{filename} を compose せずそのまま PyCompiledWgsl 化する
    (各エフェクトが個別に持っていた `load()` + PyCompiledWgsl(...) の定型を
    まとめたもの。common_dir 配下(expand.wgsl / select.wgsl)にも
    current_dir 配下(エフェクト専用のシェーダー)にも使える)。

    `output_format` を省略した場合は generator の image_format が使われる。
    """
    return PyCompiledWgsl(
        name, load_text(os.path.join(directory, filename)), aperio_plugin.image_generator, None,
        output_format=output_format,
    )


def compose_common_shader(
    name: str,
    modules: list,
    directory: str,
    filename: str,
    output_format: "gpu_util.WrappedImagePixelFormat | None" = None,
) -> PyCompiledWgsl:
    """{directory}/{filename} を modules と合成して PyCompiledWgsl 化する
    (各エフェクトが個別に持っていた compose_new(..., create_naga_module(...), ...)
    の定型をまとめたもの。common_dir 配下の共有シェーダー(box_average_dir.wgsl
    等)にも current_dir 配下のエフェクト専用シェーダーにも使える)。

    `output_format` を省略した場合は generator の image_format が使われる。
    """
    return PyCompiledWgsl.compose_new(
        name,
        modules,
        gpu_util.create_naga_module(os.path.join(directory, filename)),
        aperio_plugin.image_generator,
        output_format=output_format,
    )
