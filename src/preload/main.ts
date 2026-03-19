import { contextBridge, ipcRenderer, sharedTexture } from "electron";
import { type LayerStructure } from "native";

export type SyncedStoreState = {
  fps: number;
  viewerState: {
    state: "playing" | "paused";
    changeTime: number;
    beginFrame: number;
  };
};

// メインプロセスからMessagePortを受け取り、レンダラープロセスのwindowに転送する
ipcRenderer.on("frame-port-main", (event) => {
  const port: MessagePort = event.ports[0];
  window.postMessage({ type: "frame-port" }, "*", [port]);
});

type SharedTextureReceiverParam = Parameters<
  typeof sharedTexture.setSharedTextureReceiver
>[0];

// フレーム取得系API
contextBridge.exposeInMainWorld("frame", {
  sendPort: async () => {
    await ipcRenderer.invoke("send-port");
  },
  getFrameBuf: async (
    count: number,
    width: number,
    height: number,
    frameStruct: LayerStructure[],
  ) => {
    await ipcRenderer.invoke(
      "get-frame-buf",
      count,
      width,
      height,
      frameStruct,
    );
  },

  setReceiver: (cb: SharedTextureReceiverParam) => {
    sharedTexture.setSharedTextureReceiver(cb);
  },
  getFrameSharedTexture: async (
    count: number,
    frameStruct: LayerStructure[],
  ) => {
    await ipcRenderer.invoke("get-frame-shared-texture", count, frameStruct);
  },
});

// Rendezvous API
contextBridge.exposeInMainWorld("rendezvous", {
  register: () => ipcRenderer.invoke("rendezvous:register"),
  heartbeat: (clientId: number) =>
    ipcRenderer.invoke("rendezvous:heartbeat", clientId),
  getMaster: () => ipcRenderer.invoke("rendezvous:get-master"),
  requestState: (masterWebContentsId: number) =>
    ipcRenderer.invoke("rendezvous:request-state", masterWebContentsId),
  stateResponse: (requesterId: number, state: unknown) =>
    ipcRenderer.invoke("rendezvous:state-response", requesterId, state),
  onProvideState: (cb: (requesterWebContentsId: number) => void) => {
    const listener = (_: Electron.IpcRendererEvent, id: number) => cb(id);
    ipcRenderer.on("rendezvous:provide-state", listener);
    return () => ipcRenderer.removeListener("rendezvous:provide-state", listener);
  },
  onClientDied: (cb: (deadClientId: number) => void) => {
    const listener = (_: Electron.IpcRendererEvent, id: number) => cb(id);
    ipcRenderer.on("rendezvous:client-died", listener);
    return () => ipcRenderer.removeListener("rendezvous:client-died", listener);
  },
});

// その他のメインプロセスAPI
contextBridge.exposeInMainWorld("main", {
  getConfig: () => ipcRenderer.invoke("get-config"),
  getEventStack: () => ipcRenderer.invoke("get-event-stack-length"),
  onEventStackChanged: (cb: (length: number) => void) => {
    const listener = (_event: Electron.IpcRendererEvent, length: number) => {
      cb(length);
    };
    ipcRenderer.on("event-stack-length-changed", listener);

    return () => {
      ipcRenderer.removeListener("event-stack-length-changed", listener);
    };
  },
  resizeOsr: (width: number, height: number) =>
    ipcRenderer.invoke("resize-osr", width, height),
  showDialog: (id: string) => ipcRenderer.invoke("show-dialog", id),
});
