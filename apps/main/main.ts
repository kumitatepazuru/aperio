import { IpcChannel } from "@aperio/shared";
import {
  app,
  BrowserWindow,
  IpcMainInvokeEvent,
  ipcMain,
  screen,
  dialog,
  type OpenDialogOptions,
} from "electron";
import { SyncServer } from "./sync";
import * as path from "path";
import { fileURLToPath } from "node:url";
import { getArch, getOs } from "./getPlatform";
import { NativeModule } from "./nativeModule";
import {
  AperioConfig,
  ItemStructure,
  WrappedSharedTextureFormat,
  type SyncableStatePartial,
} from "native";
import setMenu from "./menu";
import { registerContextMenuIpc } from "./contextMenu";
import { registerPluginProtocol, registerPluginScheme } from "./pluginProtocol";

const fileName = fileURLToPath(import.meta.url);
const dirName = path.dirname(fileName);

const isDev = !app.isPackaged;
// TODO: 環境によってフラグを調整するように
app.commandLine.appendSwitch("enable-unsafe-webgpu");
app.commandLine.appendSwitch("ignore-gpu-blocklist");
// カスタムスキームの特権登録は app の ready 前に行う必要がある
registerPluginScheme();

let osrWin: BrowserWindow | null = null;
let win: BrowserWindow | null = null;
let dialogWin: BrowserWindow | null = null;

// TODO: リソースパス取得系IPCを一元化して引数で処理を分けるようにする
function getResources() {
  return isDev
    ? path.join(app.getAppPath(), "dist", "resources", `${getOs()}-${getArch()}`)
    : process.resourcesPath;
}

function getPluginManager() {
  return isDev
    ? path.join(app.getAppPath(), "python")
    : path.join(process.resourcesPath, "plmanager");
}

function getDefaultPlugins() {
  return isDev
    ? path.join(app.getAppPath(), "build", "plugins")
    : path.join(process.resourcesPath, "default-plugins");
}

function getDistDir() {
  return isDev
    ? path.join(app.getAppPath(), "dist", "app")
    : path.join(process.resourcesPath, "app.asar.unpacked", "dist", "app");
}

const nativeModule = new NativeModule({
  dataDir: app.getPath("userData"),
  localDataDir: path.join(app.getPath("userData"), "local"),
  resourceDir: getResources(),
  pluginManagerDir: getPluginManager(),
  defaultPluginsDir: getDefaultPlugins(),
  distDir: getDistDir(),
}, isDev);

const showDialog = async (id: string) => {
  if (isDev) {
    await dialogWin?.loadURL(`http://localhost:5173/dialog/?id=${id}`);
  } else {
    const dialogHtml = path.join(dirName, `./dialog.html?id=${id}`);
    await dialogWin?.loadFile(dialogHtml);
  }

  dialogWin?.show();
};

nativeModule.setEventStackListener((length) => {
  osrWin?.webContents.send(IpcChannel.eventStackLengthChanged, length);
});

ipcMain.handle(IpcChannel.sendPort, (event) => {
  nativeModule.sendPort(event.sender);
});

ipcMain.handle(
  IpcChannel.getFrameBuf,
  (
    _: IpcMainInvokeEvent,
    count: number,
    width: number,
    height: number,
    frameStruct: ItemStructure[],
  ) => {
    nativeModule.getFrameBuf(count, width, height, frameStruct);
  },
);

ipcMain.handle(
  IpcChannel.getFrameSharedTexture,
  (event, count: number, frameStruct: ItemStructure[]) => {
    nativeModule.getFrameSharedTexture(count, frameStruct, event.sender);
  },
);

ipcMain.handle(IpcChannel.getConfig, () => {
  return nativeModule.configManager.config;
});

ipcMain.handle(IpcChannel.getEventStackLength, () => {
  return nativeModule.getEventStack();
});

ipcMain.handle(
  IpcChannel.playAudio,
  (
    _,
    audioStructure: ItemStructure[],
    sampleRate: number,
    channels: number,
    startTime: number,
    duration: number,
  ) => {
    nativeModule.aperioManager.playAudio(
      audioStructure,
      sampleRate,
      channels,
      startTime,
      duration,
    );
  },
);

ipcMain.handle(IpcChannel.getPendingSamples, () => {
  return nativeModule.aperioManager.audioManager.pendingSamples;
});

ipcMain.handle(IpcChannel.stopAudio, () => {
  nativeModule.aperioManager.audioManager.stop();
});

ipcMain.handle(IpcChannel.resizeOsr, (_, width: number, height: number) => {
  if (!osrWin) return;
  // setSize()は論理ピクセルを受け取るが、coded_sizeは物理ピクセルで返されるため、
  // desired_size / scaleFactor を論理サイズとして渡すことで
  // 物理テクスチャサイズ = ユーザーが指定したフレーム解像度になる。
  const display = screen.getDisplayMatching(osrWin.getBounds());
  const sf = display.scaleFactor;
  console.log(
    `Resizing OSR window to ${width}x${height} (scale factor: ${sf})`,
  );
  osrWin.setSize(Math.round(width / sf), Math.round(height / sf), false);
});

ipcMain.handle(IpcChannel.showDialog, (_, id: string) => showDialog(id));

ipcMain.handle(IpcChannel.showOpenDialog, (_, options: OpenDialogOptions) => {
  return dialog.showOpenDialogSync(options);
});
setMenu(showDialog);
registerContextMenuIpc(nativeModule);

ipcMain.handle(IpcChannel.saveConfig, (_, config: Partial<AperioConfig>) => {
  nativeModule.saveConfig(config);
});

ipcMain.handle(IpcChannel.getPluginNames, () => {
  return nativeModule.aperioManager.getPluginNames();
});

ipcMain.handle(IpcChannel.getFontsList, () => {
  return nativeModule.aperioManager.getFontsList();
});

ipcMain.handle(
  IpcChannel.requestNew,
  (_, pluginName: string, args: Record<string, unknown>) => {
    return nativeModule.aperioManager.requestNew(pluginName, args);
  },
);

ipcMain.handle(
  IpcChannel.requestParameterStruct,
  (_, pluginName: string, params: Record<string, unknown>) => {
    return nativeModule.aperioManager.requestStructure(pluginName, params);
  },
);

// ─── Sync Server ─────────────────────────────────────────────────────────────

const syncServer = new SyncServer(nativeModule);

ipcMain.handle(IpcChannel.syncRegister, (event) => {
  return syncServer.register(event.sender.id);
});

ipcMain.handle(IpcChannel.syncSet, (event, partial: SyncableStatePartial) => {
  syncServer.setAndBroadcast(event.sender.id, partial);
});

async function createWindow() {
  const config = nativeModule.configManager.config;
  if (config.fastPreview) {
    let format: "argb" | "rgbaf16" | undefined;
    switch (config.texPixelFormat) {
      case WrappedSharedTextureFormat.Rgba16Float:
        format = "rgbaf16";
        break;
      case WrappedSharedTextureFormat.Bgra8Unorm:
        format = "argb";
        break;
      default:
        format = undefined;
        break;
    }

    osrWin = new BrowserWindow({
      width: 1920,
      height: 1080,
      show: false,
      webPreferences: {
        preload: path.join(dirName, "./preload.js"),
        offscreen: {
          useSharedTexture: true,
          sharedTexturePixelFormat: format,
        },
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: false,
      },
    });
    nativeModule.setOsrWebContents(osrWin.webContents);
    osrWin.webContents.on("did-finish-load", () => {
      osrWin?.webContents.send(
        IpcChannel.eventStackLengthChanged,
        nativeModule.getEventStack(),
      );
    });
  }

  win = new BrowserWindow({
    width: 1100,
    height: 720,
    webPreferences: {
      // 必須: preload はコンパイル後のパスを指す
      preload: path.join(dirName, "./preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  dialogWin = new BrowserWindow({
    width: 640,
    height: 480,
    parent: win,
    modal: true,
    show: false,
    webPreferences: {
      preload: path.join(dirName, "./preload.js"),
      sandbox: false,
    },
  });
  dialogWin.setMenu(null);

  if (isDev) {
    // Vite の dev サーバに接続
    const url =
      process.env.VITE_DEV_SERVER_URL ??
      "http://localhost:5173/ui/?debug";
    await win.loadURL(url);
    await osrWin?.loadURL("http://localhost:5173/osr/");
    win.webContents.openDevTools({ mode: "detach" });
    dialogWin.webContents.openDevTools({ mode: "detach" });
  } else {
    // 本番はビルド済みファイルを読む
    const indexHtml = path.join(dirName, "./ui/index.html");
    const osrHtml = path.join(dirName, "./osr/index.html");
    await win.loadFile(indexHtml);
    await osrWin?.loadFile(osrHtml);
  }

  win.on("closed", () => (win = null));

  // ダイアログは閉じるボタンを押したときshowをfalseにするだけ
  dialogWin.on("close", (e) => {
    e.preventDefault();
    dialogWin?.hide();
  });
}

app.whenReady().then(() => {
  registerPluginProtocol(nativeModule);
  return createWindow();
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});

app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0) void createWindow();
});
