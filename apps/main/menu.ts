import { app, Menu, MenuItemConstructorOptions } from "electron";

const setMenu = (showDialog: (id: string) => void) => {
  const isMac = process.platform === "darwin";
  const template: MenuItemConstructorOptions[] = [];

  const macMenu: MenuItemConstructorOptions[] = [
    {
      label: app.name,
      submenu: [
        { role: "about" },
        { type: "separator" },
        { role: "services" },
        { type: "separator" },
        { role: "hide" },
        { role: "hideOthers" },
        { role: "quit" },
      ],
    },
  ];

  if (isMac) {
    // { role: 'appMenu' }
    template.push(...macMenu);
  }
  // { role: 'fileMenu' }
  template.push({
    label: "ファイル",
    submenu: [isMac ? { role: "close" } : { role: "quit" }],
  });

  // { role: 'editMenu' }
  const editSubMenu: MenuItemConstructorOptions[] = [
    { role: "undo" },
    { role: "redo" },
    { type: "separator" },
    { role: "cut" },
    { role: "copy" },
    { role: "paste" },
  ];

  if (isMac) {
    editSubMenu.push(
      { role: "pasteAndMatchStyle" },
      { role: "delete" },
      { role: "selectAll" },
    );
  } else {
    editSubMenu.push(
      { role: "delete" },
      { type: "separator" },
      { role: "selectAll" },
    );
  }

  template.push({
    label: "Edit",
    submenu: editSubMenu,
  });

  // { role: 'viewMenu' }
  template.push({
    label: "View",
    submenu: [
      { role: "resetZoom" },
      { role: "zoomIn" },
      { role: "zoomOut" },
      { type: "separator" },
      { role: "togglefullscreen" },
    ],
  });

  template.push({
    label: "動画",
    submenu: [
      { label: "解像度の変更", click: () => showDialog("change-resolution") },
    ],
  });

  // { role: 'windowMenu' }

  const windowSubMenu: MenuItemConstructorOptions[] = [
    { role: "minimize" },
    { role: "zoom" },
  ];

  if (isMac) {
    windowSubMenu.push(
      { type: "separator" },
      { role: "front" },
      { type: "separator" },
    );
  } else {
    windowSubMenu.push({ role: "close" });
  }

  template.push({
    label: "Window",
    submenu: windowSubMenu,
  });

  template.push({
    label: "ヘルプ",
    submenu: [{ role: "toggleDevTools" }],
  });

  const menu = Menu.buildFromTemplate(template);
  Menu.setApplicationMenu(menu);
};

export default setMenu;
