import os
import site
import struct
import sys
from typing import Literal

import cv2
from gpu_util import PyCompiledWgsl, PyImageGenerator
import numpy as np

from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, ObjectGeneratorBase


class TestObject(ObjectGeneratorBase):
    """
    テストフレームを生成するオブジェクトプラグイン。OpencvとGStreamerのテストソースを利用してフレームを生成する。
    """

    frame = cv2.VideoCapture("videotestsrc ! videoconvert ! appsink")  # GStreamerのテストソースを利用

    def __init__(self, generator: PyImageGenerator):
        super().__init__(generator)
        print("--- System Information ---")
        print(f"OpenCV version: {cv2.__version__}")
        print(f"Numpy version: {np.__version__}")
        print(f"site.getsitepackages(): {site.getsitepackages()}")
        print(f"sys.executable: {sys.executable}")
        print(f"sys.path: {sys.path}")
        print("--------------------------")

        self.name = "TestObject"
        self.display_name = "Test Object"
        self.description = "This is a test object that generates frames using OpenCV and GStreamer videotestsrc."

        current_dir = os.path.dirname(__file__)
        with open(os.path.join(current_dir, "test.wgsl"), "r") as f:
            self.shader = PyCompiledWgsl("test", f.read(), generator)

    def generate(self, frame_number: int, obj_args: dict, width: int, height: int) -> GeneratorWgslReturn:
        # ret, img = self.frame.read()
        # if not ret:
        #     raise RuntimeError("Failed to read frame from videotestsrc")

        # cv2.putText(img, f"Frame: {frame_number}", (50, 50),
        #             cv2.FONT_HERSHEY_SIMPLEX, 1, (255, 255, 255), 2, cv2.LINE_AA)
        # img = cv2.resize(img, (shape[1], shape[0]))  # 指定された形状にリサイズ
        # if shape[2] == 1:
        #     img = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)  # グレースケールに変換
        #     img = img[:, :, np.newaxis]  # チャンネル次元を追加
        # elif shape[2] == 4:
        #     img = cv2.cvtColor(img, cv2.COLOR_BGR2BGRA)  # BGRAに変換

        # return img

        params = struct.pack("<i", frame_number)
        return GeneratorWgslReturn(self.shader, params, width, height)