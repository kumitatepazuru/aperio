import {
  BrowserWindow,
  ipcMain,
  Menu,
  MenuItemConstructorOptions,
} from "electron";
import { NativeModule } from "./nativeModule";

/** グローバルなコンテキストメニューレジストリ。{ id: Menu } の形で登録する */
export const contextMenus: Record<
  string,
  (
    webContents: Electron.WebContents,
    nativeModule: NativeModule,
  ) => MenuItemConstructorOptions[]
> = {
  timeline: (webContents, nativeModule) => {
    const pluginNameInfo = nativeModule.aperioManager.getPluginNames();

    return [
      {
        label: "オブジェクトを追加",
        submenu: Object.entries(pluginNameInfo.basePlugin).map(
          ([name, displayName]) => ({
            label: displayName,
            submenu: Object.entries(pluginNameInfo.objectPlugins)
              .filter(([objName]) => objName.startsWith(`${name}.`))
              .map(([objName, objDisplayName]) => ({
                label: objDisplayName,
                click: () => {
                  webContents.send("add-object", objName);
                },
              })),
          }),
        ),
      },
      { type: "separator" },
      { role: "copy" },
      { role: "paste" },
      { role: "cut" },
      { role: "selectAll" },
    ];
  },
};

export const registerContextMenuIpc = (nativeModule: NativeModule) => {
  ipcMain.handle("context-menu-open", (event, id: string) => {
    const menu = contextMenus[id];
    if (!menu) return;
    const template = menu(event.sender, nativeModule);
    const win = BrowserWindow.fromWebContents(event.sender) ?? undefined;
    Menu.buildFromTemplate(template).popup({ window: win });
  });
};
