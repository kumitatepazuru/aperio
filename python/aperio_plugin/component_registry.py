import os.path

from aperio import logger
from aperio.store import PluginComponent, StoreManager, SyncableStatePartial

from .plugin_base import MainPluginBase


class ComponentRegistry:
    """
    UI の PluginComponent に差し込む React コンポーネントを store の plugin_components に登録する。
    UI は plugin://{plugin.name}/{type}/ として読み込む。
    """

    def __init__(self, store_manager: StoreManager) -> None:
        self._store_manager = store_manager

    # TODO: typeのenum化
    def add(self, plugin: MainPluginBase, dir: str, type: str) -> None:
        """
        コンポーネントを登録する。同じ(plugin.name, type)を再登録した場合は警告を出して無視する。

        Args:
            plugin: コンポーネントを登録するメインプラグインのインスタンス (plugin.nameが pluginId になる)
            dir: ビルド済みのindex.jsがあるディレクトリのパス (絶対パスに変換される)
            type: 差し込み先の type

        Raises:
            TypeError: plugin がメインプラグイン(MainPluginBase)ではない場合
        """
        if not isinstance(plugin, MainPluginBase):
            raise TypeError("add_component() accepts only a main plugin (MainPluginBase) instance")

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

    def remove_owned_by(self, plugin_id: str) -> None:
        """指定されたメインプラグインが登録したコンポーネントをすべて取り除く(初期化に失敗したプラグインの後始末用)。"""
        current = self._store_manager.get_state().plugin_components
        remaining = [c for c in current if c.plugin_id != plugin_id]
        if len(remaining) != len(current):
            self._store_manager.set_partial(SyncableStatePartial(plugin_components=remaining))
