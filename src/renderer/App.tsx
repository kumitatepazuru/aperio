import "./App.css";
import FrameBufferRenderer from "./frameRenderer/FrameBufferRenderer";
import FrameTextureRenderer from "./frameRenderer/FrameTextureRenderer";
import Dock from "./dock/Dock";
import Timeline from "./timeline/Timeline";
import { useRef } from "react";

const config = await window.main.getConfig();

function App() {
  const rendererCanvasRef = useRef<HTMLDivElement | null>(null);

  return (
    <Dock>
      <div ref={rendererCanvasRef}>
        {config.fastPreview ? (
          <FrameTextureRenderer />
        ) : (
          <FrameBufferRenderer />
        )}
      </div>
      <Timeline />
    </Dock>
  );
}

export default App;
