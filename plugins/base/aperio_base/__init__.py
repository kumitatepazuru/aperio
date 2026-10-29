# メインプラグインは pyproject.toml の entry point (aperio.plugins) から読み込まれる。
# objects/effects はロード時にここの関数をインポートするので、このモジュールからサブモジュールを import しないこと。
_video_sync_last: tuple[int, int] | None = None


def write_video_sync_frame(frame_number: int, video_frame_number: int) -> None:
    global _video_sync_last
    _video_sync_last = (frame_number, video_frame_number)


def read_video_sync_frame(frame_number: int) -> int | None:
    if _video_sync_last is not None and _video_sync_last[0] == frame_number:
        return _video_sync_last[1]
    return None
