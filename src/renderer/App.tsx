import "./App.css";
import * as THREE from "three/webgpu";
import { Canvas } from "@react-three/fiber";
import FrameRenderer from "./FrameRenderer";
import { CameraControls, Grid, StatsGl } from "@react-three/drei";

function App() {
  return (
    <div className="w-screen h-screen">
      <div className="w-full h-full stats">
        <Canvas
          flat
          linear
          gl={async (props) => {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            const renderer = new THREE.WebGPURenderer(props as any);
            await renderer.init();
            return renderer;
          }}
        >
          <StatsGl className="stats" />
          <Grid infiniteGrid />
          <CameraControls />
          <FrameRenderer />
        </Canvas>
      </div>
    </div>
  );
}

export default App;
