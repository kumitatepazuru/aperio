import uuid

import aperio_plugin
from aperio.item_structures import GenerateStructure, GeneratorEvent, GeneratorInformation, ItemStructure
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import *

# additionalObject(ItemResult.additional_object / AudioGeneratorReturn.additional_object)から
# 参照される合成用オブジェクトの実体を、ハンドル文字列経由で横流しするための保留プール。
# エフェクトが自分の中で既に組み立てた Generator*Return を、そのまま1個のオブジェクトとして
# 合成ループに流し込みたい場合に使う(shadow の「影を別オブジェクトで描画」等)。
_pending_video: dict[str, "GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn"] = {}
_pending_audio: dict[str, AudioGeneratorReturn] = {}


def _make_generate_structure(name: str, handle: str) -> GenerateStructure:
    """GenerateStructure を新規構築するヘルパー。`__new__` は id/name/display_name しか
    受け取らず継承元の PyDict は空のまま作られるため、`layer.object["parameters"]` 等の
    dict アイテムアクセスが動くように呼び出し側で明示的に設定し直す。"""
    structure_id = str(uuid.uuid4())
    structure = GenerateStructure(id=structure_id, name=name, display_name="", parameters={"handle": handle})
    return structure


def make_deferred_video_object(
    result: "GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn",
) -> GenerateStructure:
    """既に組み立てた Generator*Return を、additionalObject 用の GenerateStructure に変換する。"""
    handle = str(uuid.uuid4())
    _pending_video[handle] = result
    return _make_generate_structure("base._deferred_video_object", handle)


def make_deferred_audio_object(result: AudioGeneratorReturn) -> GenerateStructure:
    """既に組み立てた AudioGeneratorReturn を、additionalObject 用の GenerateStructure に変換する。"""
    handle = str(uuid.uuid4())
    _pending_audio[handle] = result
    return _make_generate_structure("base._deferred_audio_object", handle)


def apply_generate_result(
    builder: PyImageGenerateBuilder,
    result: "GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn",
) -> PyImageGenerateBuilder:
    """Generator*Return を builder に適用する。`aperio_plugin._apply_generate_result` と同じスイッチを、
    プラグイン側からも使えるように複製したもの(既存の`_process_video_item`の対応表と齟齬が出ないよう、
    variant を追加した場合はあちらも合わせて更新すること)。"""
    item_result = result.item_result
    if isinstance(result, GeneratorWgslReturn):
        return builder.add_wgsl(result.compiled, result.params, item_result.width, item_result.height)
    elif isinstance(result, GeneratorFuncReturn):
        return builder.add_func(result.compiled, result.params, item_result.width, item_result.height)
    elif isinstance(result, GeneratorTextureReturn):
        return builder.add_texture_func(result.compiled, result.params, item_result.width, item_result.height)
    elif isinstance(result, GeneratorBuilderReturn):
        return builder.add_builder(result.builder)
    return builder


def rerender_owner_object(
    owner: "ItemStructure.Video", frame_number: int, width: int, height: int
) -> PyImageGenerateBuilder | None:
    """additionalObject として独立したアイテムに切り離される影などが、暗黙のパイプライン合流
    (add_parallel_wgsl 経由で前段の state を継承する仕組み)に頼れず、自前でシルエット等の
    元データ(実際にピクセルを持つ builder)を必要とする場合に使うヘルパー。
    owner の object をもう一度 generate() することで、既存の layer_frame とは独立した
    (=別アイテムの先頭ステップとしてそのまま使える)builder を作る。"""
    obj_plugin = aperio_plugin.manager.video_object_plugins.get(owner.object["name"])
    if obj_plugin is None:
        return None
    owner_params = VideoGenerateParameters(
        frame_number=frame_number,
        layer=owner,
        args=owner.object["parameters"],
        width=width,
        height=height,
    )
    result = obj_plugin.generate(owner_params)
    if result is None:
        return None
    return apply_generate_result(PyImageGenerateBuilder(), result)


class DeferredVideoObject(VideoObjectGeneratorBase):
    """additionalObject 経由で流し込まれた Generator*Return をそのまま返すだけの内部専用オブジェクト。
    UI のオブジェクト追加メニューには出さない(is_hidden=True)。"""

    def __init__(self) -> None:
        super().__init__()
        self.name = "base._deferred_video_object"
        self.display_name = "(内部)保留中の映像オブジェクト"
        self.description = "additionalObject 機構が内部で使う非表示のオブジェクトプラグイン。"
        self.is_hidden = True

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return GeneratorInformation(
            display_name=self.display_name, duration_frames=None, max_frame=None, min_frame=None, structure=[]
        )

    def generate(self, params: VideoGenerateParameters):
        return _pending_video.pop(params.args["handle"], None)


class DeferredAudioObject(AudioObjectGeneratorBase):
    """additionalObject 経由で流し込まれた AudioGeneratorReturn をそのまま返すだけの内部専用オブジェクト。
    UI のオブジェクト追加メニューには出さない(is_hidden=True)。"""

    def __init__(self) -> None:
        super().__init__()
        self.name = "base._deferred_audio_object"
        self.display_name = "(内部)保留中の音声オブジェクト"
        self.description = "additionalObject 機構が内部で使う非表示のオブジェクトプラグイン。"
        self.is_hidden = True

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return GeneratorInformation(
            display_name=self.display_name, duration_frames=None, max_frame=None, min_frame=None, structure=[]
        )

    def generate(self, params: AudioGenerateParameters):
        return _pending_audio.pop(params.args["handle"], None)
