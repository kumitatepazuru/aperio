import { contextBridge, ipcRenderer } from "electron";
import path from "path";
import { PlManager } from "native";

let plManagerSingleton: PlManager | null = null;

contextBridge.exposeInMainWorld("native", {
  getPlManager: async () => {
    if (!plManagerSingleton) {
      const userDataPath = await ipcRenderer.invoke("get-app-path", "userData");
      const resourcesPath = await ipcRenderer.invoke("get-resources");
      const pluginManagerPath = await ipcRenderer.invoke("get-plugin-manager");
      const defaultPluginsPath = await ipcRenderer.invoke("get-default-plugins");
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

    return plManagerSingleton;
  },
});

contextBridge.exposeInMainWorld("path", {
  getPath: (name: "userData" | "temp" | "exe") =>
    ipcRenderer.invoke("get-app-path", name),
  getResources: () => ipcRenderer.invoke("get-resources"),
});
