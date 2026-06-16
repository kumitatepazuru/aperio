import { webContents } from "electron";
import type { SyncableState, SyncableStatePartial } from "native";
import { NativeModule } from "./nativeModule";

export class SyncServer {
  private renderers = new Set<number>();
  nativeModule: NativeModule;

  constructor(nativeModule: NativeModule) {
    this.nativeModule = nativeModule;
  }

  register(webContentsId: number): SyncableState {
    this.renderers.add(webContentsId);
    const wc = webContents.fromId(webContentsId);
    if (wc) {
      wc.once("destroyed", () => this.renderers.delete(webContentsId));
    }
    return this.nativeModule.storeManager.getState();
  }

  setAndBroadcast(
    senderWebContentsId: number,
    partial: SyncableStatePartial,
  ): void {
    this.nativeModule.storeManager.setPartial(partial);
    for (const id of this.renderers) {
      if (id === senderWebContentsId) continue;
      const wc = webContents.fromId(id);
      if (wc && !wc.isDestroyed()) {
        wc.send("sync:diff", partial);
      } else {
        this.renderers.delete(id);
      }
    }
  }
}
