import aperio_plugin
from aperio_plugin.plugin_base import MainPluginBase
from aperio_plugin.plugin_manager import PluginManager

from .expand import ExpandEffect
from .flip import FlipEffect
from .position import PositionEffect
from .resize import ResizeEffect
from .rotation import RotationEffect
from .rotation90 import Rotation90Effect
from .transparency import TransparencyEffect
from .zoom import ZoomEffect


@PluginManager.plugin
class AperioBaseBasicEffectPlugin(MainPluginBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "basic_effect"
        self.display_name = "基本効果"
        self.description = "This plugin provides basic effects for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"
        self.num_sub_plugins = 8

        aperio_plugin.manager.register_sub_plugin(self, PositionEffect())
        aperio_plugin.manager.register_sub_plugin(self, ZoomEffect())
        aperio_plugin.manager.register_sub_plugin(self, TransparencyEffect())
        aperio_plugin.manager.register_sub_plugin(self, RotationEffect())
        aperio_plugin.manager.register_sub_plugin(self, ExpandEffect())
        aperio_plugin.manager.register_sub_plugin(self, ResizeEffect())
        aperio_plugin.manager.register_sub_plugin(self, Rotation90Effect())
        aperio_plugin.manager.register_sub_plugin(self, FlipEffect())
