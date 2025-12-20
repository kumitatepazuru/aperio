import {
  MessageChannelMain,
  MessagePortMain,
  sharedTexture,
  WebContentsPaintEventParams,
} from "electron";
import { Dirs, FrameLayerStructure, NodeOffscreenSharedTextureInfo, PlManager } from "native";

export class NativeModule {
  plManagerSingleton: PlManager;
  p1: MessagePortMain;
  p2: MessagePortMain;
  buffer: SharedArrayBuffer;
  osrWc?: Electron.WebContents;

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

  setOsrWebContents(wc: Electron.WebContents) {
    this.osrWc = wc;
    // this.osrWc.stopPainting(); // 最初は止めておく
  }

  requestOneFrame(): Promise<WebContentsPaintEventParams> {
    return new Promise((resolve, reject) => {
      if (!this.osrWc) {
        reject(new Error("OSR WebContents is not set"));
        return;
      }

      const onPaint = (event: WebContentsPaintEventParams) => {
        console.log("Paint event received");
        // すぐ止める（以降 push が来ない）
        this.osrWc?.stopPainting();

        resolve(event);
      };

      this.osrWc.on("paint", onPaint);

      // this.osrWc.startPainting(); // 描画再開
      // this.osrWc.invalidate(); // 再描画要求（次の paint を起こす）
    });
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

  async getFrameSharedTexture(
    count: number,
    frameStruct: FrameLayerStructure[],
    frame: Electron.WebContents
  ) {
    const baseTexture = await this.requestOneFrame(); // TODO: フレーム生成処理と並列化して高速化
    const textureInfo = baseTexture.texture?.textureInfo;
    console.log("getFrameSharedTexture:", baseTexture);
    if (!textureInfo) {
      throw new Error("Failed to get base shared texture");
    }
    this.plManagerSingleton.getFrameTexture(
      count,
      frameStruct,
      textureInfo as NodeOffscreenSharedTextureInfo
    );

    const imported = sharedTexture.importSharedTexture({
      textureInfo,
    });
    baseTexture.texture?.release();

    sharedTexture.sendSharedTexture({
      frame: frame.mainFrame,
      importedSharedTexture: imported,
    });
  }
}
