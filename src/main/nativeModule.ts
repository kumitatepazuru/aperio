import {
  app,
  dialog,
  MessageChannelMain,
  MessagePortMain,
  OffscreenSharedTexture,
  sharedTexture,
  WebContentsPaintEventParams,
} from "electron";
import {
  Dirs,
  ItemStructure,
  NodeOffscreenSharedTextureInfo,
  AperioManager,
  AperioConfigManager,
  AperioConfig,
} from "native";

const TIMEOUT_MS = 5000;

export class NativeModule {
  configManager: AperioConfigManager;
  aperioManager: AperioManager;
  p1: MessagePortMain;
  p2: MessagePortMain;
  eventStack: ((texture: OffscreenSharedTexture) => Promise<void>) | null =
    null;
  paintTimeout: NodeJS.Timeout | null = null;
  onEventStackChanged?: (length: number) => void;
  private _sharedBuf: ArrayBuffer | null = null;
  private _sharedBufSize = 0;

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
    this.configManager = new AperioConfigManager(dirs);
    this.aperioManager = new AperioManager(dirs, this.configManager);
  }

  setOsrWebContents(wc: Electron.WebContents) {
    wc.on("paint", async (e: WebContentsPaintEventParams) => {
      const cb = this.eventStack;
      this.eventStack = null;
      try {
        if (cb && e.texture) {
          this.notifyEventStackChanged();
          await cb?.(e.texture);
        }
      } finally {
        e.texture?.release();
        this.schedulePaintWatchdog();
      }
    });
  }

  sendPort(webContents: Electron.WebContents) {
    webContents.postMessage("frame-port-main", null, [this.p2]);
  }

  getFrameBuf(
    count: number,
    width: number,
    height: number,
    fps: number,
    frameStruct: ItemStructure[],
  ) {
    const size = width * height * 4;
    if (!this._sharedBuf || this._sharedBufSize !== size) {
      console.log("Allocating new ArrayBuffer of size:", size);
      this._sharedBuf = new ArrayBuffer(size);
      this._sharedBufSize = size;
    }
    const data = new Uint8Array(this._sharedBuf);

    const frameResults = this.aperioManager.getFrameBuf(
      data,
      count,
      width,
      height,
      fps,
      frameStruct,
    );
    this.p1.postMessage({ frame: this._sharedBuf, frameResults });
  }

  getFrameSharedTexture(
    count: number,
    fps: number,
    frameStruct: ItemStructure[],
    frame: Electron.WebContents,
  ) {
    // if (this.eventStack) return; // すでに処理中の場合は無視
    this.eventStack = async (baseTexture) => {
      const textureInfo = baseTexture.textureInfo;
      if (!textureInfo) {
        throw new Error("Failed to get base shared texture");
      }
      const frameResults = this.aperioManager.getFrameTexture(
        count,
        this.configManager.config.texPixelFormat,
        fps,
        frameStruct,
        textureInfo as NodeOffscreenSharedTextureInfo,
      );

      const imported = sharedTexture.importSharedTexture({
        textureInfo,
      });
      await sharedTexture.sendSharedTexture(
        {
          frame: frame.mainFrame,
          importedSharedTexture: imported,
        },
        frameResults,
      );

      imported.release();
    };
    this.notifyEventStackChanged();

    this.schedulePaintWatchdog();
  }

  private schedulePaintWatchdog() {
    if (this.paintTimeout) {
      clearTimeout(this.paintTimeout);
      this.paintTimeout = null;
    }

    if (!this.eventStack) {
      return;
    }

    this.paintTimeout = setTimeout(() => {
      if (this.eventStack) {
        console.error(
          `Pending paint events not fulfilled for ${TIMEOUT_MS}ms while eventStack is non-empty.`,
        );
        const dialogResult = dialog.showMessageBoxSync({
          type: "error",
          title: "aperio レンダリングエラー",
          message: "プレビュー画面のレンダリング処理がタイムアウトしました。",
          detail:
            "より安定したレンダリング設定に変更して再起動するか、このまま待機するかを選択してください。",
          buttons: ["変更して再起動", "待機"],
          defaultId: 0,
          cancelId: 1,
        });

        if (dialogResult === 0) {
          const config = this.configManager.config;
          config.fastPreview = false;
          this.configManager.setConfig(config);
          app.relaunch();
          app.exit(0);
        } else {
          this.schedulePaintWatchdog();
        }
      }
    }, TIMEOUT_MS);
  }

  getEventStack() {
    return this.eventStack ? performance.now() : 0;
  }

  setEventStackListener(listener: (length: number) => void) {
    this.onEventStackChanged = listener;
  }

  private notifyEventStackChanged() {
    this.onEventStackChanged?.(this.eventStack ? performance.now() : 0);
  }

  saveConfig(config: Partial<AperioConfig>) {
    this.configManager.setConfig({ ...this.configManager.config, ...config });
  }
}
