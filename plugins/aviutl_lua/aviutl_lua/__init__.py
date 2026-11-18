import traceback
from pathlib import Path

import aperio_plugin
from aperio import logger
from aperio_plugin.plugin_base import MainPluginBase

from .generator import LuaScriptEffect, LuaScriptGenerator, LuaScriptObject
from .script_parser import split_sections

SCRIPTS_DIR = Path(aperio_plugin.context.dirs.data_dir) / "scripts"
logger.info(f"Lua script directory: {SCRIPTS_DIR.resolve()}")

GENERATOR_CLASSES: tuple[type[LuaScriptGenerator], ...] = (LuaScriptEffect, LuaScriptObject)
"""読み込むスクリプトの種類。各クラスの ``extension`` のファイルがそのクラスのジェネレーターになる。"""

class AviutlLuaPlugin(MainPluginBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "aviutl_lua"
        self.display_name = "AviUtl互換"
        self.description = "Runs AviUtl compatible Lua scripts as video effects."
        self.version = "1.0.0"
        self.author = "Aperio"

        if not SCRIPTS_DIR.is_dir():
            logger.warning(f"{SCRIPTS_DIR} is not found.")
            return

        names: set[str] = set()
        for generator_class in GENERATOR_CLASSES:
            for path in sorted(SCRIPTS_DIR.glob(f"*{generator_class.extension}")):
                self._load_script(generator_class, path, names)

    def _load_script(self, generator_class: type[LuaScriptGenerator], path: Path, names: set[str]) -> None:
        try:
            sections = split_sections(path)
        except OSError as e:
            logger.error(f"Cannot read {path.name}: {e}")
            return

        for section in sections:
            name = generator_class.generator_name(self.name, section)
            if name in names:
                logger.warning(f"{path.name}: '@{section.display_name}' section is duplicated. Skipping.")
                continue
            try:
                generator = generator_class(name, section)
            except Exception:
                # エラーでプラグイン全体を落とさない
                logger.error(traceback.format_exc())
                logger.error(f"{path.name} ({section.display_name}): Failed to create generator. Skipping.")
                continue
            aperio_plugin.plugin_manager.add_generator(generator)
            names.add(name)
