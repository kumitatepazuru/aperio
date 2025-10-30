import threading
from typing import Literal

import cv2

from aperio_plugin.plugin_base.generator_base import ObjectGeneratorBase
from aperio_plugin.utils import ExpandedUMat

# GStreamer+OpenCVの組み合わせはスレッドセーフではないため、スレッドローカルストレージを利用して各スレッドで独立したVideoCaptureインスタンスを持つ
thread_local = threading.local()


def _get_frame() -> cv2.VideoCapture:
    if not hasattr(thread_local, "base_test_frame"):
        thread_local.base_test_frame = cv2.VideoCapture(
            "videotestsrc ! videoconvert ! appsink")  # GStreamerのテストソースを利用

    return thread_local.base_test_frame


class TestObject(ObjectGeneratorBase):
    """
    テストフレームを生成するオブジェクトプラグイン。OpencvとGStreamerのテストソースを利用してフレームを生成する。
    """

    def __init__(self):
        super().__init__()
        self.name = "TestObject"
        self.display_name = "Test Object"
        self.description = "This is a test object that generates frames using OpenCV and GStreamer videotestsrc."

    def generate(self, frame_number: int, obj_args: dict, shape: tuple[int, int, Literal[1, 3, 4]]) -> ExpandedUMat:
        ret, img = _get_frame().read()
        if not ret:
            raise RuntimeError("Failed to read frame from videotestsrc")
        # noinspection PyTypeChecker
        img = cv2.UMat(img)

        # imgにframe_numberを描画
        cv2.putText(img, f"Frame: {frame_number}", (50, 50),
                    cv2.FONT_HERSHEY_SIMPLEX, 1, (255, 255, 255), 2, cv2.LINE_AA)

        img = cv2.resize(img, (shape[1], shape[0]))  # 指定された形状にリサイズ
        if shape[2] == 1:
            img = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)  # グレースケールに変換
        elif shape[2] == 4:
            img = cv2.cvtColor(img, cv2.COLOR_BGR2BGRA)  # BGRAに変換

        return ExpandedUMat(img, shape)
