import "./App.css";
import { Canvas } from "@react-three/fiber";
import FrameRenderer from "./FrameRenderer";
import { CameraControls, Grid } from "@react-three/drei";

function App() {
  return (
    <div className="w-screen h-screen">
      <div className="w-full h-full">
        <Canvas flat linear>
          <Grid infiniteGrid />
          <CameraControls />
          <FrameRenderer />
        </Canvas>
      </div>
    </div>
  );
}

export default App;
