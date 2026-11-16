from typing import ClassVar

from aperio import logger
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import (
    VideoEffectGeneratorBase,
    VideoGenerateParameters,
    VideoGeneratorBase,
    VideoObjectGeneratorBase,
)
from lua_runtime import LuaRuntime

from .script_parser import Section, parse_parameters


class LuaScriptGenerator(VideoGeneratorBase):
    """AviUtl 形式の Lua スクリプトを実行するジェネレーターの共通実装。"""

    extension: ClassVar[str]
    """読み込むスクリプトファイルの拡張子(先頭の.(ドット)を含む)"""
    name_prefix: ClassVar[str] = ""
    """ジェネレーター名のprefix。オブジェクトとエフェクトでファイル名が同じでも、名前が衝突しないようにする"""
    duration_frames: ClassVar[int | None] = None
    """アイテムを追加したときの既定の長さ(フレーム)。None なら呼び出し側に任せる。"""

    @classmethod
    def generator_name(cls, plugin_name: str, section: Section) -> str:
        """このセクションから作られるジェネレーターの名前。"""
        name = f"{plugin_name}.{cls.name_prefix}{section.path.stem}"
        return f"{name}@{section.display_name}" if section.has_header else name

    def __init__(self, name: str, section: Section) -> None:
        super().__init__()
        self.name = name
        self.display_name = section.display_name
        self.description = f"{section.path.name} の Aviutlスクリプト"
        logger.info("successed: " + self.display_name)

        self._location = f"{section.path.name}@{section.display_name}" if section.has_header else section.path.name

        parsed = parse_parameters(section)
        self._runtime = LuaRuntime()
        self._parameters = []
        for param in parsed.parameters:
            try:
                self._runtime.add_variable(param.lua_name, param.lua_type, param.default)
            except (ValueError, TypeError) as e:
                logger.warning(f"{self._location}: cannot add variable '{param.lua_name}' to the runtime: {e}")
                continue
            self._parameters.append(param)

        self._prelude = "\n".join(parsed.prelude)
        # Lua のエラーの行番号がファイルの行番号と一致するように、先頭を空行で埋める
        self._script = "\n" * (section.first_line - 1) + "\n".join(section.lines)
        self._last_error: str | None = None

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=self.duration_frames,
            max_frame=None,
            min_frame=None,
            structure=[param.ui for param in self._parameters],
        )

    def _set_variables(self, params: VideoGenerateParameters) -> None:
        """スクリプトを実行する前に、Lua 側の変数を設定する。
        ``--track`` などのパラメータは共通でここで設定する。種別ごとの変数はサブクラスで追加する。"""
        args = params.args
        for param in self._parameters:
            value = param.convert(args[param.id]) if param.id in args else param.default
            self._runtime.set_variable(param.lua_name, value)

    def generate(self, params: VideoGenerateParameters) -> ItemResult:
        try:
            self._set_variables(params)
            if self._prelude:
                self._runtime.run(self._prelude)
            self._runtime.run(self._script)
        except Exception as e:
            # 毎フレーム同じエラーを出さないよう、内容が変わったときだけ出す
            message = f"{self._location}: {e}"
            if message != self._last_error:
                logger.error(message)
                self._last_error = message
        else:
            self._last_error = None

        # TODO: obj.draw などの描画結果を反映する
        return params.item_result


class LuaScriptEffect(LuaScriptGenerator, VideoEffectGeneratorBase):
    """AviUtl のアニメーション効果スクリプト(.anm)を実行するエフェクト。"""

    extension = ".anm"


class LuaScriptObject(LuaScriptGenerator, VideoObjectGeneratorBase):
    """AviUtl のオブジェクトスクリプト(.obj)を実行するオブジェクト。"""

    extension = ".obj"
    name_prefix = "obj."
    duration_frames = 300
