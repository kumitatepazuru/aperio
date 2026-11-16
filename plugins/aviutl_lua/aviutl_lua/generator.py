from aperio import logger
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import VideoEffectGeneratorBase, VideoGenerateParameters
from lua_runtime import LuaRuntime

from .script_parser import Section, parse_parameters


class LuaScriptEffect(VideoEffectGeneratorBase):
    """AviUtl 形式のLuaスクリプトを実行するエフェクト。"""

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
            duration_frames=None,
            max_frame=None,
            min_frame=None,
            structure=[param.ui for param in self._parameters],
        )

    def generate(self, params: VideoGenerateParameters) -> ItemResult:
        args = params.args
        try:
            for param in self._parameters:
                value = param.convert(args[param.id]) if param.id in args else param.default
                self._runtime.set_variable(param.lua_name, value)
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
