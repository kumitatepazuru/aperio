import os.path

import aperio_plugin
from aperio_plugin.plugin_base import MainPluginBase
from .audio import AudioObject
from .group import GroupObject
from .text import TextObject
from .video import VideoObject

# wheel のビルド時に frontend/group_item/dist が aperio_base/frontend/group_item に同梱される (pyproject.toml)
FRONTEND_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), "frontend")


class AperioBaseObjectPlugin(MainPluginBase):
    def __init__(self):
        super().__init__()
        self.name = "base_object"
        self.display_name = "基本オブジェクト"
        self.description = "This is a plugin that provides basic objects for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"

        plugin_manager = aperio_plugin.plugin_manager
        plugin_manager.add_generator(AudioObject())
        group = GroupObject()
        plugin_manager.add_generator(group)
        plugin_manager.add_component(group, os.path.join(FRONTEND_DIR, "group_item"), "item")
        plugin_manager.add_generator(TextObject())
        plugin_manager.add_generator(VideoObject())
