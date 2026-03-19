import type { FC, Ref } from "react";
import useFrameBufferRenderer from "./useFrameBufferRenderer";
import useFrameTextureRenderer from "./useFrameTextureRenderer";
import useStore from "@shared/store";

const config = await window.main.getConfig();

const Canvas: FC<{ ref: Ref<HTMLCanvasElement> }> = ({ ref }) => {
  const { width, height } = useStore(({ frameState }) => frameState);

  return (
    <div className="h-full w-full flex items-center justify-center">
      <canvas
        ref={ref}
        width={width}
        height={height}
        className="max-w-full max-h-full border"
      />
    </div>
  );
};

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
