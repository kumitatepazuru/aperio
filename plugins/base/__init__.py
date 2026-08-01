import aperio_plugin
from aperio_plugin.plugin_manager import PluginManager
from aperio_plugin.plugin_base import MainPluginBase


from .effects.blur.blur import BlurEffect
from .effects.boundary_blur.blur import BoundaryBlurEffect
from .effects.chroma_key.chroma_key import ChromaKeyEffect
from .effects.color_key.color_key import ColorKeyEffect
from .effects.adjustments.color import ColorAdjustmentEffect
from .effects.clip.clip import ClipEffect
from .effects.diffusion_light.diffusion_light import DiffusionLightEffect
from .effects.glint.glint import GlintEffect
from .effects.glow.glow import GlowEffect
from .effects.light.light import LightEffect
from .effects.luma_key.luma_key import LumaKeyEffect
from .effects.luminous.luminous import LuminousEffect
from .effects.mozaic.mozaic import MozaicEffect
from .objects.audio.audio import AudioObject
from .objects.text.text import TextObject
from .objects.video.video import VideoObject


@PluginManager.plugin
class AperioBasePlugin(MainPluginBase):
    def __init__(self):
        super().__init__()
        self.name = "base"
        self.display_name = "基本"
        self.description = "This is a plugin that provides basic effects/objects for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"
        self.num_sub_plugins = 16

        aperio_plugin.manager.register_sub_plugin(self, BlurEffect())
        aperio_plugin.manager.register_sub_plugin(self, BoundaryBlurEffect())
        aperio_plugin.manager.register_sub_plugin(self, ChromaKeyEffect())
        aperio_plugin.manager.register_sub_plugin(self, ColorKeyEffect())
        aperio_plugin.manager.register_sub_plugin(self, ColorAdjustmentEffect())
        aperio_plugin.manager.register_sub_plugin(self, ClipEffect())
        aperio_plugin.manager.register_sub_plugin(self, DiffusionLightEffect())
        aperio_plugin.manager.register_sub_plugin(self, GlintEffect())
        aperio_plugin.manager.register_sub_plugin(self, GlowEffect())
        aperio_plugin.manager.register_sub_plugin(self, LightEffect())
        aperio_plugin.manager.register_sub_plugin(self, LumaKeyEffect())
        aperio_plugin.manager.register_sub_plugin(self, LuminousEffect())
        aperio_plugin.manager.register_sub_plugin(self, MozaicEffect())
        aperio_plugin.manager.register_sub_plugin(self, AudioObject())
        aperio_plugin.manager.register_sub_plugin(self, TextObject())
        aperio_plugin.manager.register_sub_plugin(self, VideoObject())
