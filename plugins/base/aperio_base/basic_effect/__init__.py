import aperio_plugin
from aperio_plugin.plugin_base import MainPluginBase

from .expand import ExpandEffect
from .flip import FlipEffect
from .position import PositionEffect
from .resize import ResizeEffect
from .rotation import RotationEffect
from .rotation90 import Rotation90Effect
from .transparency import TransparencyEffect
from .zoom import ZoomEffect


class AperioBaseBasicEffectPlugin(MainPluginBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "basic_effect"
        self.display_name = "基本効果"
        self.description = "This plugin provides basic effects for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"

        aperio_plugin.plugin_manager.add_generator(PositionEffect())
        aperio_plugin.plugin_manager.add_generator(ZoomEffect())
        aperio_plugin.plugin_manager.add_generator(TransparencyEffect())
        aperio_plugin.plugin_manager.add_generator(RotationEffect())
        aperio_plugin.plugin_manager.add_generator(ExpandEffect())
        aperio_plugin.plugin_manager.add_generator(ResizeEffect())
        aperio_plugin.plugin_manager.add_generator(Rotation90Effect())
        aperio_plugin.plugin_manager.add_generator(FlipEffect())
