import { app, BrowserWindow } from "electron";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const isDev =
  process.env.VITE_DEV_SERVER_URL || process.env.ELECTRON_IS_DEV === "1";

let win: BrowserWindow | null = null;

async function createWindow() {
  win = new BrowserWindow({
    width: 1100,
    height: 720,
    webPreferences: {
      // 必須: preload はコンパイル後のパスを指す
      preload: path.join(__dirname, "../preload/main.js"),
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
    const indexHtml = path.join(__dirname, "../renderer/index.html");
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
