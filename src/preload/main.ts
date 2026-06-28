import {
  contextBridge,
  ipcRenderer,
  IpcRendererEvent,
  sharedTexture,
  type OpenDialogOptions,
} from "electron";
import { AperioConfig, type ItemStructure } from "native";

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
    frameStruct: ItemStructure[],
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
    frameStruct: ItemStructure[],
  ) => {
    await ipcRenderer.invoke("get-frame-shared-texture", count, frameStruct);
  },
});

// Audio API
contextBridge.exposeInMainWorld("audio", {
  play: (
    audioStructure: ItemStructure[],
    sampleRate: number,
    channels: number,
    startTime: number,
    duration: number,
  ) => {
    void ipcRenderer.invoke(
      "play-audio",
      audioStructure,
      sampleRate,
      channels,
      startTime,
      duration,
    );
  },
  stop: () => {
    void ipcRenderer.invoke("stop-audio");
  },
  getPendingSamples: () => {
    return ipcRenderer.invoke("get-pending-samples");
  },
});

// Sync API
contextBridge.exposeInMainWorld("sync", {
  register: () => ipcRenderer.invoke("sync:register"),
  set: (partial: object): void => {
    void ipcRenderer.invoke("sync:set", partial);
  },
  onDiff: (cb: (partial: object) => void) => {
    const listener = (_: IpcRendererEvent, partial: object) => cb(partial);
    ipcRenderer.on("sync:diff", listener);
    return () => ipcRenderer.removeListener("sync:diff", listener);
  },
});

// その他のメインプロセスAPI
contextBridge.exposeInMainWorld("main", {
  getConfig: () => ipcRenderer.invoke("get-config"),
  saveConfig: (config: Partial<AperioConfig>) =>
    ipcRenderer.invoke("save-config", config),
  getEventStack: () => ipcRenderer.invoke("get-event-stack-length"),
  onEventStackChanged: (cb: (length: number) => void) => {
    const listener = (_event: IpcRendererEvent, length: number) => {
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
  openContextMenu: (id: string) => ipcRenderer.invoke("context-menu-open", id),
  onAddObject: (cb: (objName: string, type: "Audio" | "Video") => void) => {
    const listener = (
      _event: IpcRendererEvent,
      objName: string,
      type: "Audio" | "Video",
    ) => {
      cb(objName, type);
    };
    ipcRenderer.on("add-object", listener);

    return () => {
      ipcRenderer.removeListener("add-object", listener);
    };
  },
  getPluginNames: () => ipcRenderer.invoke("get-plugin-names"),
  getFontsList: (): Promise<Record<string, number[]>> =>
    ipcRenderer.invoke("get-fonts-list"),
  requestParameterStruct: (
    pluginName: string,
    params: Record<string, unknown>,
  ) => ipcRenderer.invoke("request-parameter-struct", pluginName, params),
  requestNewGenerator: (pluginName: string, args: Record<string, unknown>) =>
    ipcRenderer.invoke("request-new", pluginName, args),
  showOpenDialog: (options: OpenDialogOptions): Promise<string[] | undefined> =>
    ipcRenderer.invoke("show-open-dialog", options),
});
