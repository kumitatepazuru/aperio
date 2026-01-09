import {
  MessageChannelMain,
  MessagePortMain,
  OffscreenSharedTexture,
  sharedTexture,
  WebContentsPaintEventParams,
} from "electron";
import {
  Dirs,
  FrameLayerStructure,
  NodeOffscreenSharedTextureInfo,
  PlManager,
} from "native";

export class NativeModule {
  plManager: PlManager;
  p1: MessagePortMain;
  p2: MessagePortMain;
  buffer: SharedArrayBuffer;
  eventStack: ((texture: OffscreenSharedTexture) => Promise<void>)[] = [];

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
    this.plManager = new PlManager(dirs);
    this.plManager.initialize();

    this.buffer = new SharedArrayBuffer(1920 * 1080 * 4); // 1920 x 1080 x 4 bytes for RGBA
  }

  setOsrWebContents(wc: Electron.WebContents) {
    wc.on("paint", async (e: WebContentsPaintEventParams) => {
      try {
        if (this.eventStack.length > 0 && e.texture) {
          const cb = this.eventStack.shift();
          await cb?.(e.texture);
        }
      } finally {
        e.texture?.release();
      }
    });
  }

  sendPort(webContents: Electron.WebContents) {
    webContents.postMessage("frame-port-main", null, [this.p2]);
  }

  getFrameBuf(count: number, frameStruct: FrameLayerStructure[]) {
    // ArrayBufferをここで作ってgetFrameに参照渡しする
    const buffer = new ArrayBuffer(1920 * 1080 * 4); // 1920 x 1080 x 4 bytes for RGBA
    const data = new Uint8Array(buffer);

    this.plManager.getFrameBuf(data, count, frameStruct);
    this.p1.postMessage(buffer);
  }

  async getFrameSharedTexture(
    count: number,
    frameStruct: FrameLayerStructure[],
    frame: Electron.WebContents
  ) {
    this.eventStack.push(async (baseTexture) => {
      const textureInfo = baseTexture.textureInfo;
      if (!textureInfo) {
        throw new Error("Failed to get base shared texture");
      }
      this.plManager.getFrameTexture(
        count,
        frameStruct,
        textureInfo as NodeOffscreenSharedTextureInfo
      );

      const imported = sharedTexture.importSharedTexture({
        textureInfo,
      });
      await sharedTexture.sendSharedTexture({
        frame: frame.mainFrame,
        importedSharedTexture: imported,
      });

      imported.release();
    });
  }
}
