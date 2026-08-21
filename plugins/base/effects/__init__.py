import aperio_plugin
from aperio_plugin.plugin_base import MainPluginBase
from aperio_plugin.plugin_manager import PluginManager
from .blur import BlurEffect
from .border import BorderEffect
from .boundary_blur import BoundaryBlurEffect
from .chroma_key import ChromaKeyEffect
from .color_key import ColorKeyEffect
from .adjustments import ColorAdjustmentEffect
from .clip import ClipEffect
from .convex_edge import ConvexEdgeEffect
from .diagonal_clipping import DiagonalClippingEffect
from .directional_blur import DirectionalBlurEffect
from .emission_blur import EmissionBlurEffect
from .lens_blur import LensBlurEffect
from .diffusion_light import DiffusionLightEffect
from .edge_extraction import EdgeExtractionEffect
from .fade import FadeEffect
from .glint import GlintEffect
from .glow import GlowEffect
from .image_loop import ImageLoopEffect
from .light import LightEffect
from .luma_key import LumaKeyEffect
from .luminous import LuminousEffect
from .mirror import MirrorEffect
from .mozaic import MozaicEffect
from .polar_coordinate_transform import PolarCoordinateTransformEffect
from .raster import RasterEffect
from .ripple import RippleEffect
from .shadow import ShadowEffect, ShadowLayerEffect
from .sharp import SharpEffect
from .vibration import VibrationEffect
from .wipe import WipeEffect


@PluginManager.plugin
class AperioBaseEffectPlugin(MainPluginBase):
    def __init__(self):
        super().__init__()
        self.name = "base_effect"
        self.display_name = "基本エフェクト"
        self.description = "This is a plugin that provides basic effects for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"
        self.num_sub_plugins = 31

        aperio_plugin.manager.register_sub_plugin(self, BlurEffect())
        aperio_plugin.manager.register_sub_plugin(self, BorderEffect())
        aperio_plugin.manager.register_sub_plugin(self, BoundaryBlurEffect())
        aperio_plugin.manager.register_sub_plugin(self, ChromaKeyEffect())
        aperio_plugin.manager.register_sub_plugin(self, ColorKeyEffect())
        aperio_plugin.manager.register_sub_plugin(self, ColorAdjustmentEffect())
        aperio_plugin.manager.register_sub_plugin(self, ClipEffect())
        aperio_plugin.manager.register_sub_plugin(self, ConvexEdgeEffect())
        aperio_plugin.manager.register_sub_plugin(self, DiagonalClippingEffect())
        aperio_plugin.manager.register_sub_plugin(self, DiffusionLightEffect())
        aperio_plugin.manager.register_sub_plugin(self, DirectionalBlurEffect())
        aperio_plugin.manager.register_sub_plugin(self, EdgeExtractionEffect())
        aperio_plugin.manager.register_sub_plugin(self, EmissionBlurEffect())
        aperio_plugin.manager.register_sub_plugin(self, FadeEffect())
        aperio_plugin.manager.register_sub_plugin(self, GlintEffect())
        aperio_plugin.manager.register_sub_plugin(self, GlowEffect())
        aperio_plugin.manager.register_sub_plugin(self, ImageLoopEffect())
        aperio_plugin.manager.register_sub_plugin(self, LensBlurEffect())
        aperio_plugin.manager.register_sub_plugin(self, LightEffect())
        aperio_plugin.manager.register_sub_plugin(self, LumaKeyEffect())
        aperio_plugin.manager.register_sub_plugin(self, LuminousEffect())
        aperio_plugin.manager.register_sub_plugin(self, MirrorEffect())
        aperio_plugin.manager.register_sub_plugin(self, MozaicEffect())
        aperio_plugin.manager.register_sub_plugin(self, PolarCoordinateTransformEffect())
        aperio_plugin.manager.register_sub_plugin(self, RasterEffect())
        aperio_plugin.manager.register_sub_plugin(self, RippleEffect())
        aperio_plugin.manager.register_sub_plugin(self, ShadowEffect())
        aperio_plugin.manager.register_sub_plugin(self, ShadowLayerEffect())
        aperio_plugin.manager.register_sub_plugin(self, SharpEffect())
        aperio_plugin.manager.register_sub_plugin(self, VibrationEffect())
        aperio_plugin.manager.register_sub_plugin(self, WipeEffect())