import re
import time
import traceback
from collections.abc import Mapping
from importlib.metadata import EntryPoint, entry_points
from types import MappingProxyType

from aperio import logger

from .component_registry import ComponentRegistry
from .generator_registry import GeneratorRegistry
from .plugin_base import GeneratorBase, MainPluginBase
from .plugin_base.generator_base import GeneratorKind
from .plugin_load_progress import LoadProgressReporter

ENTRY_POINT_GROUP = "aperio.plugins"
"""プラグインパッケージが MainPluginBase のサブクラスを公開する entry point のグループ名"""

PLUGIN_ID_PATTERN = re.compile(r"[a-z0-9._-]+")
"""メインプラグイン名(= entry point 名)に使える文字。
UI は plugin://{name}/ の host として参照し、host は小文字に正規化されるため、大文字などは使えない。"""


class PluginLoader:
    """
    uv でインストールされたプラグインパッケージを entry point から発見し、メインプラグインをインスタンス化する。
    メインプラグインは初期化中に add_generator() でジェネレーターを、add_component() でコンポーネントを登録する。
    """

    def __init__(self, registry: GeneratorRegistry, components: ComponentRegistry) -> None:
        self._registry = registry
        self._components = components
        self._plugins: dict[str, MainPluginBase] = {}
        # 読み込み中だけ値を持つ。_loading はインスタンス化中のメインプラグインの名前(= entry point 名)
        self._loading: str | None = None
        self._progress: LoadProgressReporter | None = None

    @property
    def plugins(self) -> Mapping[str, MainPluginBase]:
        """読み込み済みのメインプラグイン (entry point 名, インスタンス)"""
        return MappingProxyType(self._plugins)

    def load_all(self) -> None:
        """aperio.plugins グループの entry point をすべて読み込む。"""
        logger.info("Beginning to load plugins...")
        t = time.perf_counter()

        with LoadProgressReporter() as progress:
            self._progress = progress
            try:
                classes = self._import_entry_points(list(entry_points(group=ENTRY_POINT_GROUP)))
                progress.begin_instantiate(len(classes))
                for name, plugin_cls in classes.items():
                    progress.begin_plugin(name)
                    self._instantiate(name, plugin_cls)
            finally:
                self._progress = None

        self._log_summary(time.perf_counter() - t)

    def add_generator(self, generator: GeneratorBase) -> None:
        """
        インスタンス化中のメインプラグインにジェネレーターを登録する。

        Raises:
            RuntimeError: メインプラグインの初期化中以外に呼ばれた場合
        """
        if self._loading is None:
            raise RuntimeError("add_generator() can only be called while a main plugin is being initialized")

        self._registry.add(self._loading, generator)
        if self._progress is not None:
            self._progress.generator_added(generator)

    def _import_entry_points(self, eps: list[EntryPoint]) -> dict[str, type[MainPluginBase]]:
        assert self._progress is not None
        self._progress.begin_import(len(eps))

        classes: dict[str, type[MainPluginBase]] = {}
        for ep in eps:
            self._progress.step_import(ep.name)
            if ep.name in classes or ep.name in self._plugins:
                logger.warning(f"Plugin {ep.name} ({ep.value}) is already registered. Skipping.")
                continue

            if not PLUGIN_ID_PATTERN.fullmatch(ep.name):
                logger.error(
                    f"Invalid plugin name '{ep.name}' ({ep.value}). "
                    f"Only characters matching {PLUGIN_ID_PATTERN.pattern} are allowed. Skipping."
                )
                continue

            try:
                plugin_cls = ep.load()
            except Exception as e:
                logger.error(traceback.format_exc())
                logger.error(f"Failed to import plugin {ep.name} ({ep.value}): {e}")
                continue

            if not (isinstance(plugin_cls, type) and issubclass(plugin_cls, MainPluginBase)):
                logger.error(f"Entry point {ep.name} ({ep.value}) is not a subclass of MainPluginBase. Skipping.")
                continue
            classes[ep.name] = plugin_cls

        return classes

    def _instantiate(self, name: str, plugin_cls: type[MainPluginBase]) -> None:
        self._loading = name
        try:
            plugin = plugin_cls()
        except Exception as e:
            logger.error(traceback.format_exc())
            logger.error(f"Failed to load plugin {name}: {e}")
            self._rollback(name)
            return
        finally:
            self._loading = None

        if not PLUGIN_ID_PATTERN.fullmatch(plugin.name):
            logger.error(
                f"Invalid plugin name '{plugin.name}' (entry point: {name}). "
                f"Only characters matching {PLUGIN_ID_PATTERN.pattern} are allowed. Skipping."
            )
            self._rollback(name, plugin.name)
            return

        if plugin.name != name:
            logger.warning(f"Plugin name '{plugin.name}' differs from its entry point name '{name}'.")
        self._plugins[name] = plugin
        logger.info(f"Registered plugin: {plugin.name}")

    def _rollback(self, name: str, *component_owners: str) -> None:
        """
        初期化に失敗したメインプラグインが登録したジェネレーターとコンポーネントを取り除く。

        Args:
            name: entry point 名 (ジェネレーターの所有者)
            component_owners: name のほかに、コンポーネントの pluginId として使われたかもしれない名前 (plugin.name)
        """
        self._registry.remove_owned_by(name)
        for owner in {name, *component_owners}:
            self._components.remove_owned_by(owner)

    def _log_summary(self, elapsed: float) -> None:
        counts = ", ".join(f"{self._registry.count(kind)} {kind.display_name}" for kind in GeneratorKind)
        logger.info(f"Total plugins loaded: {len(self._plugins)}, including {counts} generators.")
        logger.info(f"it takes {elapsed:.2f} seconds.")

        logger.info("Loaded Plugins ---")
        for kind in GeneratorKind:
            logger.info(
                "\n".join(f"{g.name}({kind.display_name})- {g.get_display_info()}" for g in self._registry.of_kind(kind))
            )
