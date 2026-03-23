import os
import site
import sys

from aperio_plugin.types.frame_structure import Vec2IntParam
import cv2
from gpu_util import PyCompiledWgsl, PyImageGenerator
import numpy as np

from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, ObjectGeneratorBase


class TestObject(ObjectGeneratorBase):
    """
    テストフレームを生成するオブジェクトプラグイン。OpencvとGStreamerのテストソースを利用してフレームを生成する。
    """

    frame = cv2.VideoCapture("videotestsrc ! videoconvert ! appsink", cv2.CAP_GSTREAMER)  # GStreamerのテストソースを利用
    # frame.set(cv2.CAP_PROP_FPS, 60)

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
        
        self.request_args_struct = [
            Vec2IntParam(id="text_pos", title="テキスト位置", default_x=50, default_y=50, suffix="px"),
        ]

    def generate(self, frame_number: int, obj_args: dict, width: int, height: int) -> GeneratorWgslReturn:
        ret, img = self.frame.read()
        if not ret:
            raise RuntimeError("Failed to read frame from videotestsrc")
        position = obj_args.get("text_pos")
        if position is None:
            raise ValueError("text_pos argument is required")

        cv2.putText(img, f"Frame: {frame_number}", (position["x"], position["y"]),
                    cv2.FONT_HERSHEY_SIMPLEX, 1, (255, 255, 255), 2, cv2.LINE_AA)
        
        # float32に変換
        img = img.astype(np.float32) / 255.0        

        return GeneratorWgslReturn(self.shader, img.tobytes(), img.shape[1], img.shape[0])