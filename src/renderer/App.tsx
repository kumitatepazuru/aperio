import "./App.css";
import FrameTextureRenderer from "./frameRenderer/FrameTextureRenderer";

function App() {
  return (
    <div className="w-screen h-screen">
      <div className="w-full h-full stats">
        <FrameTextureRenderer />
      </div>
    </div>
  );
}

export default App;
