import aperio_plugin
from aperio_plugin import PluginManager
from aperio_plugin.plugin_base import MainPluginBase


from .effects.blur.blur import BlurEffect
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

        aperio_plugin.manager.register_sub_plugin(self, BlurEffect())
        aperio_plugin.manager.register_sub_plugin(self, AudioObject())
        aperio_plugin.manager.register_sub_plugin(self, TextObject())
        aperio_plugin.manager.register_sub_plugin(self, VideoObject())
