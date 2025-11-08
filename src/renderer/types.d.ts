import { FrameLayerStructure } from "native";

declare global {
  interface Window {
    frame: {
      init: () => void;
      getFrame: (count: number, frameStruct: FrameLayerStructure[]) => Promise<Uint8Array<ArrayBufferLike>>;
    },
    path: {
      getPath: (name: "userData" | "temp" | "exe") => Promise<string>;
      getResources: () => Promise<string>;
      getPluginManager: () => Promise<string>;
      getDefaultPlugins: () => Promise<string>;
      getDistDir: () => Promise<string>;
    };
  }
}
