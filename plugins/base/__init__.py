from aperio_plugin import PluginManager
from aperio_plugin.plugin_base import MainPluginBase

from .filters.blur.blur import BlurFilter
from .objects.test.test import TestObject


@PluginManager.plugin
class AperioBasePlugin(MainPluginBase):
    def __init__(self, manager, generator):
        super().__init__(manager, generator)
        self.name = "base"
        self.display_name = "基本"
        self.description = "This is a plugin that provides basic filters/objects for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"

        manager.register_sub_plugin(self, TestObject(generator))
        manager.register_sub_plugin(self, BlurFilter(generator))
