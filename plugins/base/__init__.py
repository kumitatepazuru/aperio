from aperio_plugin import PluginManager
from aperio_plugin.plugin_base import MainPluginBase

from .effects.blur.blur import BlurEffect
from .objects.test.test import TestObject


@PluginManager.plugin
class AperioBasePlugin(MainPluginBase):
    def __init__(self, manager, generator):
        super().__init__(manager, generator)
        self.name = "base"
        self.display_name = "基本"
        self.description = "This is a plugin that provides basic effects/objects for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"

        manager.register_sub_plugin(self, TestObject(generator))
        manager.register_sub_plugin(self, BlurEffect(generator))
