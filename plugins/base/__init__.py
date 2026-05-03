from aperio_plugin import PluginManager
from aperio_plugin.plugin_base import MainPluginBase


from .effects.blur.blur import BlurEffect
# from .objects.test.test import TestObject
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

        # self.manager.register_sub_plugin(self, TestObject(self.image_generator))
        self.manager.register_sub_plugin(self, BlurEffect(self.image_generator))
        self.manager.register_sub_plugin(self, TextObject(self.text_renderer))
        self.manager.register_sub_plugin(self, VideoObject(self.image_generator))
