import { Suspense } from "react";
import Dock from "./dock/Dock";
import FrameRenderer from "./frameRenderer/FrameRenderer";
import Timeline from "./timeline/Timeline";
import Loading from "@shared/Loading";
import ParameterEditor from "./parameterEditor/ParameterEditor";

function App() {
  return (
    <Suspense fallback={<Loading />}>
      <Dock>
        <FrameRenderer key="aperio.frame_renderer" />
        <Timeline key="aperio.timeline" />
        <ParameterEditor key="aperio.parameter_editor" />
      </Dock>
    </Suspense>
  );
}

export default App;
