import "./App.css";
import { Canvas } from "@react-three/fiber";
import FrameRenderer from "./FrameRenderer";
import { CameraControls, Grid, StatsGl } from "@react-three/drei";

function App() {
  return (
    <div className="w-screen h-screen">
      <div className="w-full h-full stats">
        <Canvas flat linear>
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
