import os
import site
import sys

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

        self.name = "base.test_object"
        self.display_name = "Test Object"
        self.description = "This is a test object that generates frames using OpenCV and GStreamer videotestsrc."

        current_dir = os.path.dirname(__file__)
        with open(os.path.join(current_dir, "test.wgsl"), "r") as f:
            self.shader = PyCompiledWgsl("test", f.read(), generator, None)

    def generate(self, frame_number: int, obj_args: dict, width: int, height: int) -> GeneratorWgslReturn:
        ret, img = self.frame.read()
        if not ret:
            raise RuntimeError("Failed to read frame from videotestsrc")

        cv2.putText(img, f"Frame: {frame_number}", (50, 50),
                    cv2.FONT_HERSHEY_SIMPLEX, 1, (255, 255, 255), 2, cv2.LINE_AA)
        
        # float32に変換
        img = img.astype(np.float32) / 255.0        

        return GeneratorWgslReturn(self.shader, img.tobytes(), img.shape[1], img.shape[0])