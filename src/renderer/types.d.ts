import { FrameLayerStructure } from "native";

declare global {
  interface Window {
    frame: {
      sendPort: () => Promise<void>;
      getFrameBuf: (count: number, frameStruct: FrameLayerStructure[]) => Promise<void>;
      getFrameSharedTexture: (count: number, frameStruct: FrameLayerStructure[]) => Promise<void>;
    },
  }
}
