import { MessageChannelMain, MessagePortMain } from "electron";
import { Dirs, FrameLayerStructure, PlManager } from "native";

export class NativeModule {
  plManagerSingleton: PlManager;
  p1: MessagePortMain;
  p2: MessagePortMain;
  buffer: SharedArrayBuffer;

  constructor(dirs: Dirs) {
    const { port1, port2 } = new MessageChannelMain();
    this.p1 = port1;
    this.p2 = port2;
    this.p1.start();

    console.log("Plugin Manager is being initialized");
    console.log("User Data Path:", dirs.dataDir);
    console.log("Resources Path:", dirs.resourceDir);
    console.log("Plugin Manager Path:", dirs.pluginManagerDir);
    console.log("Default Plugins Path:", dirs.defaultPluginsDir);
    console.log("Dist Path:", dirs.distDir);
    this.plManagerSingleton = new PlManager(dirs);
    this.plManagerSingleton.initialize();

    this.buffer = new SharedArrayBuffer(1920 * 1080 * 4); // 1920 x 1080 x 4 bytes for RGBA
  }

  sendPort(webContents: Electron.WebContents) {
    webContents.postMessage("frame-port-main", null, [this.p2]);
  }

  getFrameBuf(count: number, frameStruct: FrameLayerStructure[]) {
    // ArrayBufferをここで作ってgetFrameに参照渡しする
    const buffer = new ArrayBuffer(1920 * 1080 * 4); // 1920 x 1080 x 4 bytes for RGBA
    const data = new Uint8Array(buffer);

    this.plManagerSingleton.getFrameBuf(data, count, frameStruct);
    this.p1.postMessage(buffer);
  }

  // getFrameSharedTexture(count: number, frameStruct: FrameLayerStructure[]) {
  //   const textureInfo = this.plManagerSingleton.getFrameTexture(
  //     count,
  //     frameStruct
  //   );
  //   if (!textureInfo) {
  //     throw new Error("Failed to get shared texture");
  //   }

  //   sharedTexture.importSharedTexture({
  //     textureInfo,
  //   });

  //   sharedTexture.sendSharedTexture({});
  // }
}
