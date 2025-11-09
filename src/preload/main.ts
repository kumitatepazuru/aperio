import { contextBridge, ipcRenderer } from "electron";
import path from "path";
import { LayerStructure, PlManager } from "native";

let plManagerSingleton: PlManager;

const ch = new MessageChannel();
const p1 = ch.port1;
const p2 = ch.port2;
p1.start();

// TODO: 何らかの理由によりinitがされてなかったときのエラー処理

contextBridge.exposeInMainWorld("native", {
  init: async () => {
    if (!plManagerSingleton) {
      const userDataPath = await ipcRenderer.invoke("get-app-path", "userData");
      const resourcesPath = await ipcRenderer.invoke("get-resources");
      const pluginManagerPath = await ipcRenderer.invoke("get-plugin-manager");
      const defaultPluginsPath = await ipcRenderer.invoke(
        "get-default-plugins"
      );
      const distDir = await ipcRenderer.invoke("get-dist-dir");

      console.log("Plugin Manager is being initialized");
      console.log("User Data Path:", userDataPath);
      console.log("Resources Path:", resourcesPath);
      console.log("Plugin Manager Path:", pluginManagerPath);
      console.log("Default Plugins Path:", defaultPluginsPath);
      console.log("Dist Path:", distDir);
      plManagerSingleton = new PlManager({
        dataDir: userDataPath,
        localDataDir: path.join(userDataPath, "local"),
        resourceDir: resourcesPath,
        pluginManagerDir: pluginManagerPath,
        defaultPluginsDir: defaultPluginsPath,
        distDir,
      });
      plManagerSingleton.initialize();
    }

    window.postMessage({ type: "frame-port" }, "*", [p2]);
  },
  getPluginNames: (): Record<string, string>[] => {
    return plManagerSingleton?.getPluginNames() || [];
  }
});

contextBridge.exposeInMainWorld("frame", {
  getFrame: (count: number, frameStruct: LayerStructure[]) => {
    // ArrayBufferをここで作ってgetFrameに参照渡しする
    const buffer = new ArrayBuffer(1920 * 1080 * 4); // 1920 x 1080 x 4 bytes for RGBA
    const data = new Uint8Array(buffer);

    plManagerSingleton?.getFrame(data, count, frameStruct);
    p1.postMessage(buffer, [buffer]);
  },
});

contextBridge.exposeInMainWorld("path", {
  getPath: (name: "userData" | "temp" | "exe") =>
    ipcRenderer.invoke("get-app-path", name),
  getResources: () => ipcRenderer.invoke("get-resources"),
});
