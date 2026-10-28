import { IpcChannel } from "@aperio/shared";
import {
  contextBridge,
  ipcRenderer,
  IpcRendererEvent,
  sharedTexture,
  type OpenDialogOptions,
} from "electron";
import { AperioConfig, type ItemStructure } from "native";

// メインプロセスからMessagePortを受け取り、レンダラープロセスのwindowに転送する
ipcRenderer.on(IpcChannel.framePortMain, (event) => {
  const port: MessagePort = event.ports[0];
  window.postMessage({ type: "frame-port" }, "*", [port]);
});

type SharedTextureReceiverParam = Parameters<
  typeof sharedTexture.setSharedTextureReceiver
>[0];

// フレーム取得系API
contextBridge.exposeInMainWorld("frame", {
  sendPort: async () => {
    await ipcRenderer.invoke(IpcChannel.sendPort);
  },
  getFrameBuf: async (
    count: number,
    width: number,
    height: number,
    frameStruct: ItemStructure[],
  ) => {
    await ipcRenderer.invoke(
      IpcChannel.getFrameBuf,
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
    await ipcRenderer.invoke(IpcChannel.getFrameSharedTexture, count, frameStruct);
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
      IpcChannel.playAudio,
      audioStructure,
      sampleRate,
      channels,
      startTime,
      duration,
    );
  },
  stop: () => {
    void ipcRenderer.invoke(IpcChannel.stopAudio);
  },
  getPendingSamples: () => {
    return ipcRenderer.invoke(IpcChannel.getPendingSamples);
  },
});

// Sync API
contextBridge.exposeInMainWorld("sync", {
  register: () => ipcRenderer.invoke(IpcChannel.syncRegister),
  set: (partial: object): void => {
    void ipcRenderer.invoke(IpcChannel.syncSet, partial);
  },
  onDiff: (cb: (partial: object) => void) => {
    const listener = (_: IpcRendererEvent, partial: object) => cb(partial);
    ipcRenderer.on(IpcChannel.syncDiff, listener);
    return () => ipcRenderer.removeListener(IpcChannel.syncDiff, listener);
  },
});

// その他のメインプロセスAPI
contextBridge.exposeInMainWorld("main", {
  getConfig: () => ipcRenderer.invoke(IpcChannel.getConfig),
  saveConfig: (config: Partial<AperioConfig>) =>
    ipcRenderer.invoke(IpcChannel.saveConfig, config),
  getEventStack: () => ipcRenderer.invoke(IpcChannel.getEventStackLength),
  onEventStackChanged: (cb: (length: number) => void) => {
    const listener = (_event: IpcRendererEvent, length: number) => {
      cb(length);
    };
    ipcRenderer.on(IpcChannel.eventStackLengthChanged, listener);

    return () => {
      ipcRenderer.removeListener(IpcChannel.eventStackLengthChanged, listener);
    };
  },
  resizeOsr: (width: number, height: number) =>
    ipcRenderer.invoke(IpcChannel.resizeOsr, width, height),
  showDialog: (id: string) => ipcRenderer.invoke(IpcChannel.showDialog, id),
  openContextMenu: (id: string) => ipcRenderer.invoke(IpcChannel.contextMenuOpen, id),
  onAddObject: (cb: (objName: string, type: "Audio" | "Video") => void) => {
    const listener = (
      _event: IpcRendererEvent,
      objName: string,
      type: "Audio" | "Video",
    ) => {
      cb(objName, type);
    };
    ipcRenderer.on(IpcChannel.addObject, listener);

    return () => {
      ipcRenderer.removeListener(IpcChannel.addObject, listener);
    };
  },
  getPluginNames: () => ipcRenderer.invoke(IpcChannel.getPluginNames),
  getFontsList: (): Promise<Record<string, number[]>> =>
    ipcRenderer.invoke(IpcChannel.getFontsList),
  requestParameterStruct: (
    pluginName: string,
    params: Record<string, unknown>,
  ) => ipcRenderer.invoke(IpcChannel.requestParameterStruct, pluginName, params),
  requestNewGenerator: (pluginName: string, args: Record<string, unknown>) =>
    ipcRenderer.invoke(IpcChannel.requestNew, pluginName, args),
  showOpenDialog: (options: OpenDialogOptions): Promise<string[] | undefined> =>
    ipcRenderer.invoke(IpcChannel.showOpenDialog, options),
});
