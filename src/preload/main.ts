import { contextBridge, ipcRenderer, sharedTexture } from "electron";
import { FrameLayerStructure } from "native";

ipcRenderer.on("frame-port-main", (event) => {
  const port: MessagePort = event.ports[0];
  window.postMessage({ type: "frame-port" }, "*", [port]);
});

type SharedTextureReceiverParam = Parameters<typeof sharedTexture.setSharedTextureReceiver>[0];

contextBridge.exposeInMainWorld("frame", {
  sendPort: async () => {
    await ipcRenderer.invoke("send-port");
  },
  getFrameBuf: async (count: number, frameStruct: FrameLayerStructure[]) => {
    await ipcRenderer.invoke("get-frame-buf", count, frameStruct);
  },

  setReceiver: (cb: SharedTextureReceiverParam) => {
    sharedTexture.setSharedTextureReceiver(cb);
  },
  subtle: sharedTexture.subtle,
  getFrameSharedTexture: async (
    count: number,
    frameStruct: FrameLayerStructure[]
  ) => {
    await ipcRenderer.invoke("get-frame-shared-texture", count, frameStruct);
  },
});
