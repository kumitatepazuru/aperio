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

    const getSubMenu= (pluginList: Record<string, string>, type: "Audio" | "Video") => Object.entries(pluginNameInfo.basePlugin).map(
          ([name, displayName]) => ({
            label: displayName,
            submenu: Object.entries(pluginList)
              .filter(([objName]) => objName.startsWith(`${name}.`))
              .map(([objName, objDisplayName]) => ({
                label: objDisplayName,
                click: () => {
                  webContents.send("add-object", objName, type);
                },
              })),
          }),
        );

    return [
      {
        label: "オブジェクトを追加",
        submenu: [
          {
            label: "動画",
            submenu: getSubMenu(pluginNameInfo.videoObjectPlugins, "Video"),
          },
          {
            label: "音声",
            submenu: getSubMenu(pluginNameInfo.audioObjectPlugins, "Audio"),
          },
        ]
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
