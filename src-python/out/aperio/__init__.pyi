from _typeshed import Incomplete
from aperio.audio import AudioManager
from aperio.config_manager import ConfigManager
from aperio.store import StoreManager
from typing import final

@final
class PyManagers:
    @property
    def audio_manager(self, /) -> AudioManager: ...
    @property
    def config_manager(self, /) -> ConfigManager: ...
    @property
    def store_manager(self, /) -> StoreManager: ...

def __getattr__(name: str) -> Incomplete: ...
