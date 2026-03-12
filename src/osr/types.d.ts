import type { SyncedStoreState } from "../preload/main";

declare global {
  interface Window {
    storeSync: {
      sendStoreState: (state: SyncedStoreState) => void;
      onStoreState: (cb: (state: SyncedStoreState) => void) => void;
    };
  }
}
