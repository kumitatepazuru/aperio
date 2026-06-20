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

const fileName = fileURLToPath(import.meta.url);
const dirName = path.dirname(fileName);

const isDev = !app.isPackaged;
// TODO: 環境によってフラグを調整するように
app.commandLine.appendSwitch("enable-unsafe-webgpu");
app.commandLine.appendSwitch("ignore-gpu-blocklist");

let osrWin: BrowserWindow | null = null;
let win: BrowserWindow | null = null;
let dialogWin: BrowserWindow | null = null;

// TODO: リソースパス取得系IPCを一元化して引数で処理を分けるようにする
function getResources() {
  return isDev
    ? path.join(app.getAppPath(), "resources", `${getOs()}-${getArch()}`)
    : process.resourcesPath;
}

function getPluginManager() {
  return isDev
    ? path.join(app.getAppPath(), "src-python", "src")
    : path.join(process.resourcesPath, "plmanager");
}

function getDefaultPlugins() {
  return isDev
    ? path.join(app.getAppPath(), "plugins")
    : path.join(process.resourcesPath, "default-plugins");
}

function getDistDir() {
  return isDev
    ? path.join(app.getAppPath(), "dist")
    : path.join(process.resourcesPath, "app.asar.unpacked", "dist");
}

const nativeModule = new NativeModule({
  dataDir: app.getPath("userData"),
  localDataDir: path.join(app.getPath("userData"), "local"),
  resourceDir: getResources(),
  pluginManagerDir: getPluginManager(),
  defaultPluginsDir: getDefaultPlugins(),
  distDir: getDistDir(),
});

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
  osrWin?.webContents.send("event-stack-length-changed", length);
});

ipcMain.handle("send-port", (event) => {
  nativeModule.sendPort(event.sender);
});

ipcMain.handle(
  "get-frame-buf",
  (
    _: IpcMainInvokeEvent,
    count: number,
    width: number,
    height: number,
    fps: number,
    frameStruct: ItemStructure[],
  ) => {
    nativeModule.getFrameBuf(count, width, height, fps, frameStruct);
  },
);

ipcMain.handle(
  "get-frame-shared-texture",
  (event, count: number, frameStruct: ItemStructure[]) => {
    nativeModule.getFrameSharedTexture(count, frameStruct, event.sender);
  },
);

ipcMain.handle("get-config", () => {
  return nativeModule.configManager.config;
});

ipcMain.handle("get-event-stack-length", () => {
  return nativeModule.getEventStack();
});

ipcMain.handle("resize-osr", (_, width: number, height: number) => {
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

ipcMain.handle("show-dialog", (_, id: string) => showDialog(id));

ipcMain.handle("show-open-dialog", (_, options: OpenDialogOptions) => {
  return dialog.showOpenDialogSync(options);
});
setMenu(showDialog);
registerContextMenuIpc(nativeModule);

ipcMain.handle("save-config", (_, config: Partial<AperioConfig>) => {
  nativeModule.saveConfig(config);
});

ipcMain.handle("get-plugin-names", () => {
  return nativeModule.aperioManager.getPluginNames();
});

ipcMain.handle("get-fonts-list", () => {
  return nativeModule.aperioManager.getFontsList();
});

ipcMain.handle(
  "request-new",
  (_, pluginName: string, args: Record<string, unknown>) => {
    return nativeModule.aperioManager.requestNew(
      pluginName,
      args,
    );
  },
);

ipcMain.handle(
  "request-parameter-struct",
  (_, pluginName: string, params: Record<string, unknown>) => {
    return nativeModule.aperioManager.requestStructure(
      pluginName,
      params,
    );
  },
);

// ─── Sync Server ─────────────────────────────────────────────────────────────

const syncServer = new SyncServer(nativeModule);

ipcMain.handle("sync:register", (event) => {
  return syncServer.register(event.sender.id);
});

ipcMain.handle("sync:set", (event, partial: SyncableStatePartial) => {
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
        "event-stack-length-changed",
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
      "http://localhost:5173/renderer/?debug";
    await win.loadURL(url);
    await osrWin?.loadURL("http://localhost:5173/osr/");
    win.webContents.openDevTools({ mode: "detach" });
    dialogWin.webContents.openDevTools({ mode: "detach" });
  } else {
    // 本番はビルド済みファイルを読む
    const indexHtml = path.join(dirName, "./renderer/index.html");
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

app.whenReady().then(createWindow);

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});

app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0) void createWindow();
});
