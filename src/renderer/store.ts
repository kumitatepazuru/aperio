import type { FrameLayerStructure } from "native";
import { create } from "zustand";

type TimelineLayerStructure = FrameLayerStructure & {
  id: string; // UUIDが期待される
  from: number; // 開始フレーム
  to: number; // 終了フレーム
};

type ViewerState = "playing" | "paused";

type Store = {
  frameCount: number;
  viewerState: ViewerState;
  setViewerState: (state: ViewerState) => void;
  setFrameCount: (count: number) => void;
  timelineLayers: TimelineLayerStructure[];
  setTimelineLayers: (layers: TimelineLayerStructure[]) => void;
  pluginNames?: { [key: string]: string }[];
  getPluginNames: () => { [key: string]: string }[];
};

const useStore = create<Store>()((set, get) => ({
  frameCount: 0,
  viewerState: "playing",
  setViewerState: (state) => set({ viewerState: state }),
  setFrameCount: (count) => set({ frameCount: count }),
  timelineLayers: [],
  setTimelineLayers: (layers) => set({ timelineLayers: layers }),
  pluginNames: undefined,
  getPluginNames: () => {
    // なかったら取得してセットする
    let names = get().pluginNames;
    if (!names) {
      names = window.native.getPluginNames();
      set({ pluginNames: names });
    }
    return names;
  },
}));

export default useStore;
