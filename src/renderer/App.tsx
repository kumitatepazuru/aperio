import FrameBufferRenderer from "./frameRenderer/FrameBufferRenderer";
import FrameTextureRenderer from "./frameRenderer/FrameTextureRenderer";
import Dock from "./dock/Dock";
import Timeline from "./timeline/Timeline";

const config = await window.main.getConfig();

function App() {
  return (
    <Dock>
      {config.fastPreview ? <FrameTextureRenderer /> : <FrameBufferRenderer />}
      <Timeline />
    </Dock>
  );
}

export default App;
