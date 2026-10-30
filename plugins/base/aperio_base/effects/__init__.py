import aperio_plugin
from aperio_plugin.plugin_base import MainPluginBase
from .blur import BlurEffect
from .border import BorderEffect
from .boundary_blur import BoundaryBlurEffect
from .chroma_key import ChromaKeyEffect
from .color_key import ColorKeyEffect
from .adjustments import ColorAdjustmentEffect
from .clip import ClipEffect
from .composite_image import CompositeImageEffect
from .composite_video import CompositeVideoEffect
from .convert_gamut import ConvertGamutEffect
from .convex_edge import ConvexEdgeEffect
from .deinterlacing import DeinterlacingEffect
from .diagonal_clipping import DiagonalClippingEffect
from .directional_blur import DirectionalBlurEffect
from .emission_blur import EmissionBlurEffect
from .lens_blur import LensBlurEffect
from .diffusion_light import DiffusionLightEffect
from .edge_extraction import EdgeExtractionEffect
from .fade import FadeEffect
from .glint import GlintEffect
from .glow import GlowEffect
from .gradation import GradationEffect
from .image_loop import ImageLoopEffect
from .light import LightEffect
from .luma_key import LumaKeyEffect
from .luminous import LuminousEffect
from .mirror import MirrorEffect
from .monochromatic import MonochromaticEffect
from .mozaic import MozaicEffect
from .noise import NoiseEffect
from .polar_coordinate_transform import PolarCoordinateTransformEffect
from .raster import RasterEffect
from .rgb_split import RgbSplitEffect
from .ripple import RippleEffect
from .shadow import ShadowEffect, ShadowLayerEffect
from .sharp import SharpEffect
from .vibration import VibrationEffect
from .wipe import WipeEffect


class AperioBaseEffectPlugin(MainPluginBase):
    def __init__(self):
        super().__init__()
        self.name = "base_effect"
        self.display_name = "基本エフェクト"
        self.description = "This is a plugin that provides basic effects for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"

        aperio_plugin.plugin_manager.add_generator(BlurEffect())
        aperio_plugin.plugin_manager.add_generator(BorderEffect())
        aperio_plugin.plugin_manager.add_generator(BoundaryBlurEffect())
        aperio_plugin.plugin_manager.add_generator(ChromaKeyEffect())
        aperio_plugin.plugin_manager.add_generator(ColorKeyEffect())
        aperio_plugin.plugin_manager.add_generator(ColorAdjustmentEffect())
        aperio_plugin.plugin_manager.add_generator(ClipEffect())
        aperio_plugin.plugin_manager.add_generator(CompositeImageEffect())
        aperio_plugin.plugin_manager.add_generator(CompositeVideoEffect())
        aperio_plugin.plugin_manager.add_generator(ConvertGamutEffect())
        aperio_plugin.plugin_manager.add_generator(ConvexEdgeEffect())
        aperio_plugin.plugin_manager.add_generator(DeinterlacingEffect())
        aperio_plugin.plugin_manager.add_generator(DiagonalClippingEffect())
        aperio_plugin.plugin_manager.add_generator(DiffusionLightEffect())
        aperio_plugin.plugin_manager.add_generator(DirectionalBlurEffect())
        aperio_plugin.plugin_manager.add_generator(EdgeExtractionEffect())
        aperio_plugin.plugin_manager.add_generator(EmissionBlurEffect())
        aperio_plugin.plugin_manager.add_generator(FadeEffect())
        aperio_plugin.plugin_manager.add_generator(GlintEffect())
        aperio_plugin.plugin_manager.add_generator(GlowEffect())
        aperio_plugin.plugin_manager.add_generator(GradationEffect())
        aperio_plugin.plugin_manager.add_generator(ImageLoopEffect())
        aperio_plugin.plugin_manager.add_generator(LensBlurEffect())
        aperio_plugin.plugin_manager.add_generator(LightEffect())
        aperio_plugin.plugin_manager.add_generator(LumaKeyEffect())
        aperio_plugin.plugin_manager.add_generator(LuminousEffect())
        aperio_plugin.plugin_manager.add_generator(MirrorEffect())
        aperio_plugin.plugin_manager.add_generator(MonochromaticEffect())
        aperio_plugin.plugin_manager.add_generator(MozaicEffect())
        aperio_plugin.plugin_manager.add_generator(NoiseEffect())
        aperio_plugin.plugin_manager.add_generator(PolarCoordinateTransformEffect())
        aperio_plugin.plugin_manager.add_generator(RasterEffect())
        aperio_plugin.plugin_manager.add_generator(RgbSplitEffect())
        aperio_plugin.plugin_manager.add_generator(RippleEffect())
        aperio_plugin.plugin_manager.add_generator(ShadowEffect())
        aperio_plugin.plugin_manager.add_generator(ShadowLayerEffect())
        aperio_plugin.plugin_manager.add_generator(SharpEffect())
        aperio_plugin.plugin_manager.add_generator(VibrationEffect())
        aperio_plugin.plugin_manager.add_generator(WipeEffect())