/**
 * Electron IPCのチャンネル名。main(ipcMain)とpreload(ipcRenderer)の双方がこの定数を参照することで、
 * 文字列リテラルの食い違いを防ぐ。
 */
export const IpcChannel = {
  sendPort: "send-port",
  framePortMain: "frame-port-main",
  getFrameBuf: "get-frame-buf",
  getFrameSharedTexture: "get-frame-shared-texture",
  playAudio: "play-audio",
  stopAudio: "stop-audio",
  getPendingSamples: "get-pending-samples",
  syncRegister: "sync:register",
  syncSet: "sync:set",
  syncDiff: "sync:diff",
  getConfig: "get-config",
  saveConfig: "save-config",
  getEventStackLength: "get-event-stack-length",
  eventStackLengthChanged: "event-stack-length-changed",
  resizeOsr: "resize-osr",
  showDialog: "show-dialog",
  showOpenDialog: "show-open-dialog",
  contextMenuOpen: "context-menu-open",
  addObject: "add-object",
  getPluginNames: "get-plugin-names",
  getFontsList: "get-fonts-list",
  requestParameterStruct: "request-parameter-struct",
  requestNew: "request-new",
} as const;

export type IpcChannelName = (typeof IpcChannel)[keyof typeof IpcChannel];
