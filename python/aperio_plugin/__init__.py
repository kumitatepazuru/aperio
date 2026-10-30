from aperio import PyManagers
from aperio import logger

# https://stackoverflow.com/questions/42339034/python-module-in-dist-packages-vs-site-packages
# どうやらDebian系Linuxではsite-packagesではなくdist-packagesにインストールされるらしいのでimportされない。
# また、OS管理のPythonを使っているとPYTHONHOMEを設定しているのにもかかわらずそれが適用されないケースが多い。
try:
    import numpy as np
    import numpy.typing as npt
except ImportError as e:
    import traceback as _tb
    logger.error(_tb.format_exc())

    raise ImportError(
        "Failed to import required modules. Make sure numpy are installed."
        "\n--- For Developer ---\nThis error was occured by many complicated reasons. Ensure the below check list and fix them:"
        "\n1. Make sure numpy are installed in the python environment used by Aperio."
        "\n  Running Environment can be checked in below debug info. Generally, required packages should be installed during post running process."
        "\n2. If you are using OS managed python (like apt install python3 on Debian/Ubuntu) to compile and run Aperio, it may cause this error."
        "\n  Please try to install python separately (recommends uv) with `uv python install --reinstall --no-managed-python` and run `bun moon run python:copy`."
        "\n3. For Linux: Make sure that libpython is preloaded as RTLD_GLOBAL correctly. In linux, libpython must be able to be seen globally because of policy of manylinux."
        "\n  Try add the environment LD_PRELOAD to specify the path to libpython3.x.so explicitly."
    ) from e

from .context import AperioContext
from .plugin_manager import PluginManager

# モジュールレベルグローバル — initialize() でのみ設定される。プラグインからはこの2つだけを参照する
context: AperioContext
plugin_manager: PluginManager


def initialize(managers: PyManagers) -> PluginManager:
    """
    プラグイン実行環境を初期化し、インストール済みのプラグインを読み込む。Rust 側から呼ばれる。

    プラグインは読み込み中に aperio_plugin.plugin_manager/aperio_plugin.contextを参照するため、グローバルを設定してからプラグインを読み込む。
    """
    global context, plugin_manager

    context = AperioContext.from_managers(managers)
    plugin_manager = PluginManager(context)
    plugin_manager.load_plugins()
    return plugin_manager
