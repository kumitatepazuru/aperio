import os
import site
import sys

from aperio_plugin.types.frame_structure import ColorParam, Vec2IntParam
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
            Vec2IntParam(id="text_pos", title="テキスト位置", default_value=(50, 50), suffix="px"),
            ColorParam(id="text_color", title="テキスト色", default_value=(1.0, 1.0, 1.0, 1.0), use_alpha=False),
        ]

    def generate(self, frame_number: int, obj_args: dict, width: int, height: int) -> GeneratorWgslReturn:
        ret, img = self.frame.read()
        if not ret:
            raise RuntimeError("Failed to read frame from videotestsrc")
        position = obj_args.get("text_pos")
        text_color = obj_args.get("text_color")
        if position is None:
            raise ValueError("text_pos argument is required")
        if text_color is None:
            raise ValueError("text_color argument is required")
        text_color = [int(c * 255) for c in text_color]  # RGBAを整数に変換

        cv2.putText(img, f"Frame: {frame_number}", (position[0], position[1]),
                    cv2.FONT_HERSHEY_SIMPLEX, 1, (text_color[2], text_color[1], text_color[0]), 2, cv2.LINE_AA) # OpenCVはBGR形式なので、テキストカラーをBGRの順番で指定
        
        # float32に変換
        img = img.astype(np.float32) / 255.0        

        return GeneratorWgslReturn(self.shader, img.tobytes(), img.shape[1], img.shape[0])