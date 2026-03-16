import type { FC, Ref } from "react";
import useFrameBufferRenderer from "./useFrameBufferRenderer";
import useFrameTextureRenderer from "./useFrameTextureRenderer";

const FRAME_WIDTH = 1920;
const FRAME_HEIGHT = 1080;

const config = await window.main.getConfig();

const Canvas: FC<{ ref: Ref<HTMLCanvasElement> }> = ({ ref }) => (
  <div className="h-full w-full flex items-center justify-center">
    <canvas
      ref={ref}
      width={FRAME_WIDTH}
      height={FRAME_HEIGHT}
      className="max-w-full max-h-full border"
    />
  </div>
);

const FrameTextureRenderer = () => {
  const canvasRef = useFrameTextureRenderer();

  return <Canvas ref={canvasRef} />;
};

const FrameBufferRenderer = () => {
  const canvasRef = useFrameBufferRenderer();

  return <Canvas ref={canvasRef} />;
};

const FrameRenderer = () => {
  return config.fastPreview ? (
    <FrameTextureRenderer />
  ) : (
    <FrameBufferRenderer />
  );
};

export default FrameRenderer;
