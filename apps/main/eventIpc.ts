import { EVENT_METHODS, eventChannel } from "@aperio/shared";
import { ipcMain } from "electron";
import { NativeModule } from "./nativeModule";

/** EVENT_METHODS の各メソッドを、同名の AperioManager のメソッドへ転送するIPCハンドラーとして一括登録する */
export const registerEventIpc = (nativeModule: NativeModule) => {
  for (const method of EVENT_METHODS) {
    ipcMain.handle(eventChannel(method), (_, ...args: unknown[]) => {
      const manager = nativeModule.aperioManager;
      return (manager[method] as (...args: unknown[]) => unknown).apply(
        manager,
        args,
      );
    });
  }
};
