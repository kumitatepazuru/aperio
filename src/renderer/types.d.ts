import { type FrameLayerStructure, type AppConfig } from "native";
import { sharedTexture } from "electron";
import type { SyncedStoreState } from "../preload/main";

declare global {
  interface Window {
    frame: {
      sendPort: () => Promise<void>;
      getFrameBuf: (
        count: number,
        frameStruct: FrameLayerStructure[],
      ) => Promise<void>;
      setReceiver: (
        cb: Parameters<typeof sharedTexture.setSharedTextureReceiver>[0],
      ) => void;
      getFrameSharedTexture: (
        count: number,
        frameStruct: FrameLayerStructure[],
      ) => Promise<void>;
    };
    main: {
      getPluginNames: () => Record<string, string>[];
      getConfig: () => Promise<AppConfig>;
    };
    storeSync: {
      sendStoreState: (state: SyncedStoreState) => void;
      onStoreState: (cb: (state: SyncedStoreState) => void) => void;
    };
  }
}
