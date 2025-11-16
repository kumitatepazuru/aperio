import "./App.css";
import * as THREE from "three/webgpu";
import { Canvas } from "@react-three/fiber";
// import FrameBufferRenderer from "./frameRenderer/FrameBufferRenderer";
import { CameraControls, Grid, StatsGl } from "@react-three/drei";
import FrameTextureRenderer from "./frameRenderer/FrameTextureRenderer";

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
            console.log("WebGPU initialized:", renderer);
            return renderer;
          }}
        >
          <StatsGl className="stats" />
          <Grid infiniteGrid />
          <CameraControls />
          {/* <FrameBufferRenderer /> */}
          <FrameTextureRenderer />
        </Canvas>
      </div>
    </div>
  );
}

export default App;
