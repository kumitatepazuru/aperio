# `from . import objects`/`from . import effects` より前で定義する必要がある
# (それらのモジュールがロード時にこの関数をインポートするため、循環インポートを避ける)。
_video_sync_last: tuple[int, int] | None = None


def write_video_sync_frame(frame_number: int, video_frame_number: int) -> None:
    global _video_sync_last
    _video_sync_last = (frame_number, video_frame_number)


def read_video_sync_frame(frame_number: int) -> int | None:
    if _video_sync_last is not None and _video_sync_last[0] == frame_number:
        return _video_sync_last[1]
    return None


from . import basic_effect
from . import effects
from . import objects

# TODO: パッケージの概念を作って複数プラグインの一括インストールができるようにしたい
