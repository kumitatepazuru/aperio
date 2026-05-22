import { webContents } from "electron";
import { storeGetState, storeSetPartial } from "native";
import type { SyncableState, SyncableStatePartial } from "native";

export class SyncServer {
  private renderers = new Set<number>();

  register(webContentsId: number): SyncableState {
    this.renderers.add(webContentsId);
    const wc = webContents.fromId(webContentsId);
    if (wc) {
      wc.once("destroyed", () => this.renderers.delete(webContentsId));
    }
    return storeGetState();
  }

  setAndBroadcast(
    senderWebContentsId: number,
    partial: SyncableStatePartial,
  ): void {
    storeSetPartial(partial);
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
