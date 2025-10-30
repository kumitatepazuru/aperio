from aperio_plugin import PluginManager
from aperio_plugin.plugin_base import MainPluginBase
from .objects.test import TestObject


@PluginManager.plugin
class AperioBasePlugin(MainPluginBase):
    def __init__(self, manager):
        super().__init__(manager)
        self.name = "AperioBasePlugin"
        self.display_name = "Aperio Base Plugin"
        self.description = "This is a plugin that provides basic filters/objects for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"

        manager.register_sub_plugin(
            TestObject()
        )
        print(f"{self.display_name} initialized.")
