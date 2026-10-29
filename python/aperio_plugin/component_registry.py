import os.path

from aperio import logger
from aperio.store import PluginComponent, StoreManager, SyncableStatePartial

from .plugin_base import PluginBase


class ComponentRegistry:
    """
    UI の PluginComponent に差し込む React コンポーネントを store の plugin_components に登録する。
    UI は plugin://{plugin.name}/{type}/ として読み込む。
    """

    def __init__(self, store_manager: StoreManager) -> None:
        self._store_manager = store_manager

    # TODO: typeのenum化
    def add(self, plugin: PluginBase, dir: str, type: str) -> None:
        """
        コンポーネントを登録する。同じ(plugin.name, type)を再登録した場合は警告を出して無視する。

        Args:
            plugin: コンポーネントを登録するプラグインのインスタンス (plugin.nameが pluginId になる)
            dir: ビルド済みのindex.jsがあるディレクトリのパス (絶対パスに変換される)
            type: 差し込み先の type
        """
        dir = os.path.abspath(dir)
        if not os.path.isfile(os.path.join(dir, "index.js")):
            logger.warning(f"Plugin {plugin.name}: index.js not found in '{dir}'. Component '{type}' is not registered.")
            return

        current = self._store_manager.get_state().plugin_components
        if any(c.plugin_id == plugin.name and c.component_type == type for c in current):
            logger.warning(f"Plugin {plugin.name}: Component '{type}' is already registered. Skipping.")
            return
        current.append(PluginComponent(plugin.name, type, dir))
        self._store_manager.set_partial(SyncableStatePartial(plugin_components=current))
