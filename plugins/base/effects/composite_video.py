import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import FileFilter, GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from .. import read_video_sync_frame, write_video_sync_frame
from ..common.params import make_generator_information, pack_expand_params
from ..common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader
from ..common.video_cache import VideoLoaderCache

_MEDIA_MODE_INDEX = {"overwrite_color": 0, "luma_overwrite": 1, "luma_multiply": 2}

_VIDEO_EXTENSIONS = ["mp4", "mkv", "webm", "avi", "flv", "mpeg", "mpg", "mov", "wmv", "mts", "m2ts", "m4v"]


class CompositeVideoEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.composite_video_effect"
        self.display_name = "動画ファイル合成"
        self.description = "Extracts one frame of a video file and composites it onto the object below."

        _, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")
        math_module = lib_module(common_dir, "math")

        self.expand_shader = shared_shader("expand", common_dir, "expand.wgsl")
        self.resize_bilinear_shader = compose_common_shader(
            "resize_bilinear", [math_module], common_dir, "resize_bilinear.wgsl"
        )
        self.tile_shader = shared_shader("tile", common_dir, "tile.wgsl")
        self.media_composite_mode_shader = compose_common_shader(
            "media_composite_mode", [color_module], common_dir, "media_composite_mode.wgsl"
        )

        self.video_cache = VideoLoaderCache("composite_video_frame")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, params: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.File(
                    id="video_path",
                    title="参照ファイル",
                    multi_selections=False,
                    open_type="file",
                    filters=[FileFilter("動画ファイル", _VIDEO_EXTENSIONS)],
                ),
                RequestStructureParameter.Int(
                    id="playback_start", title="再生位置", default_value=1, suffix="frame", min=1
                ),
                RequestStructureParameter.Float(
                    id="playback_speed", title="再生速度", default_value=100.0, suffix="%", min=-2000.0, max=2000.0
                ),
                RequestStructureParameter.Int(id="x", title="X", default_value=0, suffix="px"),
                RequestStructureParameter.Int(id="y", title="Y", default_value=0, suffix="px"),
                RequestStructureParameter.Float(
                    id="scale", title="拡大率", default_value=100.0, suffix="%", min=0.0, max=800.0
                ),
                RequestStructureParameter.Bool(id="loop_playback", title="ループ再生", default_value=False),
                RequestStructureParameter.Bool(
                    id="sync_video_files", title="動画ファイルの同期", default_value=False
                ),
                RequestStructureParameter.Bool(id="tile_image", title="ループ画像", default_value=False),
                RequestStructureParameter.List(
                    id="mode",
                    title="合成モード",
                    values={
                        "overwrite_color": "色情報を上書き",
                        "luma_overwrite": "輝度をアルファ値として上書き",
                        "luma_multiply": "輝度をアルファ値として乗算",
                    },
                    default_value="overwrite_color",
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        paths = args.get("video_path", [])
        path = paths[0] if paths else ""
        entry = self.video_cache.get_or_load(params.structure_id, path) if path else None
        if entry is None:
            return None
        loader, video_func = entry

        playback_start = max(1, int(args.get("playback_start", 1)))
        playback_speed = float(args.get("playback_speed", 100.0)) / 100.0
        x = int(args.get("x", 0))
        y = int(args.get("y", 0))
        scale_pct = max(0.0, float(args.get("scale", 100.0)))
        loop_playback = bool(args.get("loop_playback", False))
        sync_video_files = bool(args.get("sync_video_files", False))
        tile_image = bool(args.get("tile_image", False))
        mode = args.get("mode", "overwrite_color")

        elapsed = params.frame_number - params.layer.start
        own_frame = playback_start + round(elapsed * playback_speed)

        if sync_video_files:
            synced = read_video_sync_frame(params.frame_number)
            video_frame = playback_start + synced if synced is not None else own_frame
        else:
            video_frame = own_frame
            write_video_sync_frame(params.frame_number, video_frame)

        w, h = params.width, params.height
        fps = aperio_plugin.store_manager.get_state().frame_state.fps

        if loader.frame_count > 0:
            # avloaderのフレーム番号は「project fps基準」で解釈される
            # (avloader_video_decode_frameがtarget_time = start + (frame_num-1)/target_fpsで
            # 変換する)が、loader.frame_countは動画のnative fpsでのフレーム数なので、
            # 比較前にproject fps相当の単位へ変換する必要がある。これをしないと、
            # 動画のnative fpsがproject fpsより低いときに実際の長さより早く
            # 再生が終わってしまう。
            native_fps = loader.fps
            frame_count_at_fps = (
                loader.frame_count / native_fps * fps if native_fps > 0 else float(loader.frame_count)
            )
            limit = max(1, round(frame_count_at_fps))
            if loop_playback:
                video_frame = ((video_frame - 1) % limit) + 1
            elif video_frame < 1 or video_frame > limit:
                return None

        scale = scale_pct / 100.0
        target_w = max(1, round(loader.width * scale))
        target_h = max(1, round(loader.height * scale))

        media_branch = gpu_util.PyImageGenerateBuilder().add_texture_func(
            video_func, (video_frame, fps), loader.width, loader.height
        )
        if target_w != loader.width or target_h != loader.height:
            media_branch = media_branch.add_wgsl(
                self.resize_bilinear_shader, struct.pack("ii", target_w, target_h), target_w, target_h
            )

        if tile_image:
            media_branch = media_branch.add_wgsl(self.tile_shader, struct.pack("iiii", x, y, w, h), w, h)
        else:
            media_branch = media_branch.add_wgsl(self.expand_shader, pack_expand_params(x, y, w, h), w, h)

        dst_branch = gpu_util.PyImageGenerateBuilder()
        mode_index = _MEDIA_MODE_INDEX.get(mode, 0)
        builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_parallel_wgsl([media_branch, dst_branch])
            .add_wgsl(self.media_composite_mode_shader, struct.pack("i", mode_index), w, h)
        )

        return GeneratorBuilderReturn(builder, ItemResult(w, h))
