import { app, BrowserWindow, IpcMainInvokeEvent, ipcMain } from "electron";
import * as path from "path";
import { fileURLToPath } from "node:url";
import { getArch, getOs } from "./getPlatform";
import { NativeModule } from "./nativeModule";
import { LayerStructure, NodeSharedTextureFormat } from "native";

const fileName = fileURLToPath(import.meta.url);
const dirName = path.dirname(fileName);

const isDev = !app.isPackaged;
// TODO: 環境によってフラグを調整するように
app.commandLine.appendSwitch("enable-unsafe-webgpu");
app.commandLine.appendSwitch("ignore-gpu-blocklist");

let osrWin: BrowserWindow | null = null;
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
  (_: IpcMainInvokeEvent, count: number, frameStruct: LayerStructure[]) => {
    nativeModule.getFrameBuf(count, frameStruct);
  },
);

ipcMain.handle(
  "get-frame-shared-texture",
  async (event, count: number, frameStruct: LayerStructure[]) => {
    await nativeModule.getFrameSharedTexture(count, frameStruct, event.sender);
  },
);

ipcMain.handle("get-config", () => {
  return nativeModule.configManager.config;
});

ipcMain.on("store-state-update", (_event, state) => {
  osrWin?.webContents.send("store-state", state);
});

async function createWindow() {
  const config = nativeModule.configManager.config;
  if (config.fastPreview) {
    let format: "argb" | "rgbaf16" | undefined;
    switch (config.texPixelFormat) {
      case NodeSharedTextureFormat.Rgba16Float:
        format = "rgbaf16";
        break;
      case NodeSharedTextureFormat.Bgra8Unorm:
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

  if (isDev) {
    // Vite の dev サーバに接続
    const url =
      process.env.VITE_DEV_SERVER_URL ?? "http://localhost:5173/renderer/";
    await win.loadURL(url);
    await osrWin?.loadURL("http://localhost:5173/osr/");
    win.webContents.openDevTools({ mode: "detach" });
  } else {
    // 本番はビルド済みファイルを読む
    const indexHtml = path.join(dirName, "./renderer/index.html");
    const osrHtml = path.join(dirName, "./osr/index.html");
    await win.loadFile(indexHtml);
    await osrWin?.loadFile(osrHtml);
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
