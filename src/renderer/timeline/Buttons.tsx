import useStore, { getCurrentFrameCount } from "@shared/store";
import { FaBackward, FaForward, FaPause, FaPlay } from "react-icons/fa";
import { useShallow } from "zustand/shallow";

const Buttons = () => {
  const { state, play, pause } = useStore(
    useShallow((state) => ({
      state: state.viewerState,
      play: state.play,
      pause: state.pause,
    })),
  );

  const handlePlayPause = async () => {
    if (state.state === "playing") {
      pause();
    } else {
      play(await getCurrentFrameCount());
    }
  };

  return (
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
  );
};

export default Buttons;
