from dataclasses import dataclass

from aperio import Dirs, PyManagers, gpu_util, text_rendering
from aperio.audio import AudioManager
from aperio.config_manager import ConfigManager
from aperio.store import StoreManager


@dataclass(frozen=True)
class AperioContext:
    """
    Rust 側から受け取ったマネージャー群・ディレクトリ情報と、それをもとに生成した GPU/テキスト描画まわりをまとめたもの。
    """

    dirs: Dirs
    config_manager: ConfigManager
    store_manager: StoreManager
    audio_manager: AudioManager
    image_generator: gpu_util.PyImageGenerator
    text_renderer: text_rendering.PyTextRenderer

    @classmethod
    def from_managers(cls, managers: PyManagers, dirs: Dirs) -> "AperioContext":
        config_manager = managers.config_manager
        image_generator = gpu_util.PyImageGenerator(config_manager.config.image_pixel_format)
        return cls(
            dirs=dirs,
            config_manager=config_manager,
            store_manager=managers.store_manager,
            audio_manager=managers.audio_manager,
            image_generator=image_generator,
            text_renderer=text_rendering.PyTextRenderer(image_generator),
        )
