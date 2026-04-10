import os
import site
import sys

from aperio.frame_structure import RequestStructureParameter
import cv2
from aperio.gpu_util import PyCompiledWgsl, PyImageGenerator
import numpy as np

from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, NewObjectGeneratorReturn, ObjectGeneratorBase


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
        self.display_name = "テストオブジェクト"
        self.description = "This is a test object that generates frames using OpenCV and GStreamer videotestsrc."

        current_dir = os.path.dirname(__file__)
        with open(os.path.join(current_dir, "test.wgsl"), "r") as f:
            self.shader = PyCompiledWgsl("test", f.read(), generator, None)

        self.base_structure: list[RequestStructureParameter] = [
            RequestStructureParameter.Bool("draw_text", "フレームテキストを描画", False),
        ]

    def on_new(self, args: dict) -> NewObjectGeneratorReturn:
        return NewObjectGeneratorReturn(display_name=self.display_name, duration_frames=300, structure=self.base_structure)
    
    def on_request_structure(self, params: dict) -> list[RequestStructureParameter]:
        if params.get("draw_text", False):
            return self.base_structure + [
            RequestStructureParameter.Vec2Int("text_pos", "テキスト位置", (50, 50), "px"),
            RequestStructureParameter.Color("text_color", "テキスト色", (1.0, 1.0, 1.0, 1.0), False),
        ]
        else:
            return self.base_structure
        

    def generate(self, frame_number: int, args: dict, width: int, height: int) -> GeneratorWgslReturn:
        ret, img = self.frame.read()
        if not ret:
            raise RuntimeError("Failed to read frame from videotestsrc")
        
        if args.get("draw_text", False):
            position = args.get("text_pos", [50, 50])
            text_color = args.get("text_color", (1.0, 1.0, 1.0, 1.0))
            text_color = [int(c * 255) for c in text_color]  # RGBAを整数に変換

            cv2.putText(img, f"Frame: {frame_number}", (position[0], position[1]),
                        cv2.FONT_HERSHEY_SIMPLEX, 1, (text_color[2], text_color[1], text_color[0]), 2, cv2.LINE_AA) # OpenCVはBGR形式なので、テキストカラーをBGRの順番で指定
        
        # float32に変換
        img = img.astype(np.float32) / 255.0        

        return GeneratorWgslReturn(self.shader, img.tobytes(), img.shape[1], img.shape[0])