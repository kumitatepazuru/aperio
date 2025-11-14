import { Canvas } from "@react-three/fiber";
import Dock from "./dock/Dock";
import FrameRenderer from "./FrameRenderer";
import Timeline from "./timeline/Timeline";
import { CameraControls, Grid, StatsGl } from "@react-three/drei";
import { useRef } from "react";

function App() {
  const rendererCanvasRef = useRef<HTMLDivElement | null>(null);

  return (
    <Dock>
      <div ref={rendererCanvasRef}>
        <Canvas flat linear>
          <StatsGl
            parent={rendererCanvasRef as React.RefObject<HTMLDivElement>}
          />
          <Grid infiniteGrid />
          <CameraControls />
          <FrameRenderer />
        </Canvas>
      </div>
      <Timeline />
    </Dock>
  );
}

export default App;
