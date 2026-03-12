import useStore from "@/store";
import { FaBackward, FaForward, FaPause, FaPlay } from "react-icons/fa";
import { useShallow } from "zustand/shallow";

const Timeline = () => {
  const { state, getCurrentFrameCount, play, pause } = useStore(
    useShallow((state) => ({
      state: state.viewerState,
      getCurrentFrameCount: state.getCurrentFrameCount,
      play: state.play,
      pause: state.pause,
    }))
  );

  const handlePlayPause = () => {
    if (state.state === "playing") {
      pause();
    } else {
      play(getCurrentFrameCount());
    }
  };

  return (
    <div className="flex flex-col">
      <div className="flex gap-2 justify-center">
        <button className="btn btn-circle">
          <FaBackward />
        </button>
        <button className="btn btn-circle" onClick={handlePlayPause}>
          {state.state === "playing" ? <FaPause /> : <FaPlay />}
        </button>
        <button className="btn btn-circle">
          <FaForward />
        </button>
      </div>
    </div>
  );
};

export default Timeline;
