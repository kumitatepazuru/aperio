from typing import final

@final
class AperioConfig: ...

@final
class ConfigManager:
    @property
    def config(self, /) -> AperioConfig: ...
    @config.setter
    def config(self, /, config: AperioConfig) -> None: ...
