import type { FC, Ref } from "react";
import useFrameBufferRenderer from "./useFrameBufferRenderer";
import useFrameTextureRenderer from "./useFrameTextureRenderer";
import useStore from "@shared/store";
import Buttons from "./Buttons";
import Overlay from "./Overlay";

const config = await window.main.getConfig();

const Canvas: FC<{ ref: Ref<HTMLCanvasElement> }> = ({ ref }) => {
  const { width, height } = useStore(({ frameState }) => frameState);

  return (
    <div className="h-full w-full flex items-center justify-center @container-[size]">
      <div
        className="relative"
        style={{
          width: `min(100cqw, 100cqh * ${width} / ${height})`,
          height: `min(100cqh, 100cqw * ${height} / ${width})`,
        }}
      >
        <Overlay />
        <canvas
          ref={ref}
          width={width}
          height={height}
          className="w-full h-full border"
        />
      </div>
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
  return (
    <div className="flex flex-col gap-3 h-full">
      <div className="flex-1 min-h-0">
        {config.fastPreview ? (
          <FrameTextureRenderer />
        ) : (
          <FrameBufferRenderer />
        )}
      </div>
      <div>
        <Buttons />
      </div>
    </div>
  );
};

export default FrameRenderer;
