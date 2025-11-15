import { app, BrowserWindow, IpcMainInvokeEvent, ipcMain } from "electron";
import * as path from "path";
import { fileURLToPath } from "node:url";
import { getArch, getOs } from "./getPlatform";
import { NativeModule } from "./nativeModule";
import { FrameLayerStructure } from "native";

const fileName = fileURLToPath(import.meta.url);
const dirName = path.dirname(fileName);

const isDev = !app.isPackaged;
app.commandLine.appendSwitch("enable-unsafe-webgpu");

let win: BrowserWindow | null = null;

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

ipcMain.handle("send-port", (event) => {
  nativeModule.sendPort(event.sender);
});

ipcMain.handle(
  "get-frame-buf",
  (
    _: IpcMainInvokeEvent,
    count: number,
    frameStruct: FrameLayerStructure[]
  ) => {
    nativeModule.getFrameBuf(count, frameStruct);
  }
);

// ipcMain.handle(
//   "get-frame-shared-texture",
//   (
//     _: IpcMainInvokeEvent,
//     count: number,
//     frameStruct: FrameLayerStructure[]
//   ) => {
//     nativeModule.getFrameSharedTexture(count, frameStruct);
//   }
// );

async function createWindow() {
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

  if (isDev) {
    // Vite の dev サーバに接続
    const url = process.env.VITE_DEV_SERVER_URL ?? "http://localhost:5173";
    await win.loadURL(url);
    win.webContents.openDevTools({ mode: "detach" });
  } else {
    // 本番はビルド済みファイルを読む
    const indexHtml = path.join(dirName, "./renderer/index.html");
    await win.loadFile(indexHtml);
  }

  win.on("closed", () => (win = null));
}

app.whenReady().then(createWindow);

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});

app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0) void createWindow();
});
