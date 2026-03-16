import Dock from "./dock/Dock";
import FrameRenderer from "./frameRenderer/FrameRenderer";
import Timeline from "./timeline/Timeline";

function App() {
  return (
    <Dock>
      <FrameRenderer />
      <Timeline />
    </Dock>
  );
}

export default App;
