from typing import ClassVar

import aperio_plugin
from aperio import logger
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, ItemStructure
from aperio.store import FrameState
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import (
    VideoEffectGeneratorBase,
    VideoGenerateParameters,
    VideoGeneratorBase,
    VideoObjectGeneratorBase,
)
from lua_runtime import LuaRuntime

from .obj_variables import WRITABLE_VARIABLES, find_root_id, find_timeline_item, scale_from, zoom_aspect
from .obj_variables import register as register_obj_variables
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

        self.location = f"{section.path.name}@{section.display_name}" if section.has_header else section.path.name

        parsed = parse_parameters(section)
        self._runtime = LuaRuntime()
        # スクリプトのパラメータと名前が衝突したら、パラメータ側を警告して捨てる
        register_obj_variables(self._runtime)
        self.parameters = []
        for param in parsed.parameters:
            try:
                self._runtime.add_variable(param.lua_name, param.lua_type, param.default)
            except (ValueError, TypeError) as e:
                logger.warning(f"{self.location}: cannot add variable '{param.lua_name}' to the runtime: {e}")
                continue
            self.parameters.append(param)

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
            structure=[param.ui for param in self.parameters],
        )

    def _warn(self, message: str) -> None:
        """警告を出す"""
        message = f"{self.location}: {message}"
        logger.warning(message)

    def _base_item(self, params: VideoGenerateParameters, timeline_items: list[ItemStructure]) -> ItemStructure.Video | None:
        """
        表示基準座標(obj.x, obj.y)を取るアイテム。params.itemはイベントで加工されている可能性があるので、
        タイムライン上のアイテムを使う。追加アイテムの場合は生成元を遡ったアイテムを使う。
        生成元が見つからない場合は None(処理をskip)。
        """
        item_id = params.item.id
        if params.parent_id is not None:
            root_id = find_root_id(params.parent_id, params.item_results)
            if root_id is None:
                self._warn(f"parent item '{params.parent_id}' of '{item_id}' was not found. Skipping.")
                return None
            item_id = root_id

        item = find_timeline_item(item_id, timeline_items)
        if item is None:
            # 今後 BeforeMakeFrame で id を変えるものが出てきたときのための措置
            self._warn(f"item '{item_id}' was not found in the timeline. Using the item passed to the generator instead.")
            return params.item
        return item

    def _image_size(self, params: VideoGenerateParameters) -> tuple[float, float]:
        """obj.w, obj.h に入れる画像サイズ。"""
        return 0.0, 0.0

    def _set_variables(self, params: VideoGenerateParameters, base: ItemStructure.Video, frame_state: FrameState) -> None:
        """スクリプトを実行する前に、Lua 側の変数を設定する。"""
        rt = self._runtime
        result = params.item_result

        # 表示基準座標。ItemStructure に z は無いので 0
        base_pos = (base.x, base.y, 0.0)
        for axis, b, p in zip("xyz", base_pos, result.pos):
            rt.set_variable(f"obj.{axis}", b)
            rt.set_variable(f"obj.o{axis}", p - b)
        for axis, r in zip("xyz", result.rotate):
            rt.set_variable(f"obj.r{axis}", r)
        rt.set_variable("obj.cx", result.center_x)
        rt.set_variable("obj.cy", result.center_y)
        # TODO: ItemResult に center_z が無いので、常に 0 で計算する
        rt.set_variable("obj.cz", 0.0)
        zoom, aspect = zoom_aspect(*result.scale)
        rt.set_variable("obj.zoom", zoom)
        rt.set_variable("obj.aspect", aspect)
        rt.set_variable("obj.alpha", result.alpha)

        w, h = self._image_size(params)
        rt.set_variable("obj.w", w)
        rt.set_variable("obj.h", h)
        rt.set_variable("obj.screen_w", frame_state.width)
        rt.set_variable("obj.screen_h", frame_state.height)

        fps = frame_state.fps
        frame = params.frame_number - params.item.start
        total_frame = params.item.end - params.item.start
        rt.set_variable("obj.framerate", fps)
        rt.set_variable("obj.frame", frame)
        rt.set_variable("obj.time", frame / fps if fps > 0 else 0.0)
        rt.set_variable("obj.totalframe", total_frame)
        rt.set_variable("obj.totaltime", total_frame / fps if fps > 0 else 0.0)
        rt.set_variable("obj.layer", params.item.layer + 1)

        args = params.args
        for param in self.parameters:
            value = param.convert(args[param.id]) if param.id in args else param.default
            rt.set_variable(param.lua_name, value)

    def _read_back(self, params: VideoGenerateParameters, base: ItemStructure.Video) -> ItemResult:
        """スクリプトが書き換えた obj.* 変数を ItemResult に反映したものを返す。"""
        values = {name: float(self._runtime.get_variable(name)) for name in WRITABLE_VARIABLES}
        before = params.item_result
        result = before.copy()

        result.pos = (base.x + values["obj.ox"], base.y + values["obj.oy"], 0.0 + values["obj.oz"])
        result.rotate = (values["obj.rx"], values["obj.ry"], values["obj.rz"])
        result.center_x = values["obj.cx"]
        result.center_y = values["obj.cy"]
        # TODO: ItemResult に center_z が無いので、obj.cz は反映しない
        result.alpha = values["obj.alpha"]
        # 負の拡大率などは zoom/aspect との往復で壊れるので、変わったときだけ計算し直す
        zoom, aspect = values["obj.zoom"], values["obj.aspect"]
        if (zoom, aspect) != zoom_aspect(*before.scale):
            result.scale = scale_from(zoom, aspect)
        return result

    def generate(self, params: VideoGenerateParameters) -> ItemResult:
        state = aperio_plugin.context.store_manager.get_state()
        base = self._base_item(params, state.timeline_items)
        if base is None:
            return params.item_result

        try:
            self._set_variables(params, base, state.frame_state)
            if self._prelude:
                self._runtime.run(self._prelude)
            self._runtime.run(self._script)
            result = self._read_back(params, base)
        except Exception as e:
            # 毎フレーム同じエラーを出さないよう、内容が変わったときだけ出す
            message = f"{self.location}: {e}"
            if message != self._last_error:
                logger.error(message)
                self._last_error = message
            return params.item_result

        self._last_error = None
        # TODO: obj.draw などの描画結果を反映する
        return result


class LuaScriptEffect(LuaScriptGenerator, VideoEffectGeneratorBase):
    """AviUtl のアニメーション効果スクリプト(.anm)を実行するエフェクト。"""

    extension = ".anm"

    def _image_size(self, params: VideoGenerateParameters) -> tuple[float, float]:
        sx, sy = params.item_result.scale
        return params.width * sx, params.height * sy


class LuaScriptObject(LuaScriptGenerator, VideoObjectGeneratorBase):
    """AviUtl のオブジェクトスクリプト(.obj)を実行するオブジェクト。"""

    extension = ".obj"
    name_prefix = "obj."
    duration_frames = 300
