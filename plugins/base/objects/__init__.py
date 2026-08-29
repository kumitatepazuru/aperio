import aperio_plugin
from aperio_plugin.plugin_manager import PluginManager
from aperio_plugin.plugin_base import MainPluginBase
from .audio import AudioObject
from .text import TextObject
from .video import VideoObject


@PluginManager.plugin
class AperioBaseObjectPlugin(MainPluginBase):
    def __init__(self):
        super().__init__()
        self.name = "base_object"
        self.display_name = "基本オブジェクト"
        self.description = "This is a plugin that provides basic objects for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"
        self.num_sub_plugins = 3

        aperio_plugin.manager.register_sub_plugin(self, AudioObject())
        aperio_plugin.manager.register_sub_plugin(self, TextObject())
        aperio_plugin.manager.register_sub_plugin(self, VideoObject())
