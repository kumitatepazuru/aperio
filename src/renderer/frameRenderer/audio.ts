import {
  getCurrentAudioItems,
  getCurrentFrameCount,
  getStoreState,
} from "@shared/store";

const playAudio = async () => {
  const storeState = await getStoreState();
  const isPlaying = storeState.viewerState.state === "playing";
  const fpsSec = 1 / storeState.frameState.fps;

  if (
    !isPlaying ||
    (await window.audio.getPendingSamples()) <
      storeState.audioState.sampleRate * 0.5
  ) {
    window.audio.play(
      await getCurrentAudioItems(isPlaying ? storeState.frameState.fps : 1),
      storeState.audioState.sampleRate,
      storeState.audioState.channels,
      (await getCurrentFrameCount()) / storeState.frameState.fps, // 現在の再生位置
      isPlaying ? 1 : fpsSec, // 再生時間（秒） - 再生中は1秒分、停止中はフレーム1枚分
    );
  }
  if (
    !isPlaying &&
    (await window.audio.getPendingSamples()) >
      storeState.audioState.sampleRate * fpsSec
  ) {
    window.audio.stop();
  }
};

export default playAudio;
