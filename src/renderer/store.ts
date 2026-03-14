import { v4 as uuidv4 } from "uuid";
import type { LayerStructure } from "native";
import { create } from "zustand";

export type TimelineLayerStructure = LayerStructure & {
  id: string; // UUIDが期待される
  layer: number; // レイヤー番号
  from: number; // 開始フレーム
  to: number; // 終了フレーム
};

type ViewerState = {
  state: "playing" | "paused";
  changeTime: number; // 状態変更時刻のタイムスタンプ performance.now()基準
  beginFrame: number; // 再生開始フレーム
};

type Store = {
  fps: number;
  viewerState: ViewerState;
  play: (beginFrame: number) => void;
  pause: () => void;
  getPluginNames: () => { [key: string]: string }[];
  getCurrentFrameCount: () => number;
  getFrameStruct: () => LayerStructure[];
  setFrameCount: (frame: number) => void;
  setFps: (fps: number) => void;
  timelineLayers: TimelineLayerStructure[];
  setTimelineLayers: (layers: TimelineLayerStructure[]) => void;
  pluginNames?: { [key: string]: string }[];
};

const useStore = create<Store>()((set, get) => ({
  fps: 60,
  viewerState: {
    state: "paused",
    changeTime: performance.now(),
    beginFrame: 0,
  },
  play: (beginFrame: number) =>
    set({
      viewerState: {
        state: "playing",
        changeTime: performance.now(),
        beginFrame,
      },
    }),
  pause: () =>
    set({
      viewerState: {
        state: "paused",
        changeTime: performance.now(),
        beginFrame: get().getCurrentFrameCount(),
      },
    }),
  getCurrentFrameCount: () => {
    const { viewerState, fps } = get();
    if (viewerState.state === "playing") {
      const elapsedTime = (performance.now() - viewerState.changeTime) / 1000;
      return viewerState.beginFrame + Math.floor(elapsedTime * fps);
    } else {
      return viewerState.beginFrame;
    }
  },
  getFrameStruct: () => {
    const currentFrame = get().getCurrentFrameCount();
    return get()
      .timelineLayers.filter((layer) => {
        return currentFrame >= layer.from && currentFrame <= layer.to;
      })
      .sort((a, b) => a.layer - b.layer);
  },
  setFrameCount: (frame) =>
    set({
      viewerState: {
        state: "paused",
        changeTime: performance.now(),
        beginFrame: frame,
      },
    }),
  setFps: (fps) => set({ fps }),
  timelineLayers: [
    {
      id: uuidv4(),
      layer: 0,
      from: 0,
      to: 1000,
      x: 500,
      y: 500,
      scale: 3.0,
      rotation: 40.0,
      alpha: 1.0,
      obj: {
        name: "TestObject",
        parameters: {},
      },
      effects: [],
    },
  ],
  setTimelineLayers: (layers) => set({ timelineLayers: layers }),
  pluginNames: undefined,
  getPluginNames: () => {
    // なかったら取得してセットする
    let names = get().pluginNames;
    if (!names) {
      names = window.main.getPluginNames();
      set({ pluginNames: names });
    }
    return names;
  },
}));

// renderer -> osr へのストア同期
useStore.subscribe((state) => {
  window.storeSync?.sendStoreState({
    fps: state.fps,
    viewerState: state.viewerState,
  });
});

export default useStore;
