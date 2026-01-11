import "./App.css";
import FrameBufferRenderer from "./frameRenderer/FrameBufferRenderer";
import FrameTextureRenderer from "./frameRenderer/FrameTextureRenderer";

const config = await window.main.getConfig();

function App() {
  return (
    <div className="w-screen h-screen">
      <div className="w-full h-full stats">
        {config.fastPreview ? (
          <FrameTextureRenderer />
        ) : (
          <FrameBufferRenderer />
        )}
      </div>
    </div>
  );
}

export default App;
