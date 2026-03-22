import { Suspense } from "react";
import Dock from "./dock/Dock";
import FrameRenderer from "./frameRenderer/FrameRenderer";
import Timeline from "./timeline/Timeline";
import Loading from "@shared/Loading";

function App() {
  return (
    <Suspense fallback={<Loading />}>
      <Dock>
        <FrameRenderer key="aperio.frame_renderer" />
        <Timeline key="aperio.timeline" />
      </Dock>
    </Suspense>
  );
}

export default App;
