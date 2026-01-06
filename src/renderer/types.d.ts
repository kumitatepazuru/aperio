import { FrameLayerStructure } from "native";
import { sharedTexture } from "electron";

declare global {
  interface Window {
    frame: {
      sendPort: () => Promise<void>;
      getFrameBuf: (
        count: number,
        frameStruct: FrameLayerStructure[]
      ) => Promise<void>;
      setReceiver: (
        cb: Parameters<typeof sharedTexture.setSharedTextureReceiver>[0]
      ) => void;
      getFrameSharedTexture: (
        count: number,
        frameStruct: FrameLayerStructure[]
      ) => Promise<void>;
    };
  }
}
