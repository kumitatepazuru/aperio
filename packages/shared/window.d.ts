import {
  type AperioConfig,
  type AperioManager,
  type PluginNameInfo,
  type ItemStructure,
} from "native";
import type { EventMethod } from "./index";

/** フォントファミリー名 → ウェイト値（100/200/…/900）の配列 */
type FontsList = Record<string, number[]>;
import { sharedTexture, type OpenDialogOptions } from "electron";
import type { SyncableState, SyncableStatePartial } from "native";

// APIは自動で一括定義する
type EventApi = {
  [K in EventMethod]: (
    ...args: Parameters<AperioManager[K]>
  ) => Promise<ReturnType<AperioManager[K]>>;
};

declare global {
  interface Window {
    sync: {
      register: () => Promise<SyncableState>;
      set: (partial: SyncableStatePartial) => void;
      onDiff: (cb: (partial: SyncableStatePartial) => void) => () => void;
    };
    frame: {
      sendPort: () => Promise<void>;
      getFrameBuf: (
        count: number,
        width: number,
        height: number,
        frameStruct: ItemStructure[],
      ) => Promise<void>;
      setReceiver: (
        cb: Parameters<typeof sharedTexture.setSharedTextureReceiver>[0],
      ) => void;
      getFrameSharedTexture: (
        count: number,
        frameStruct: ItemStructure[],
      ) => Promise<void>;
    };
    audio: {
      play: (
        audioStructure: ItemStructure[],
        sampleRate: number,
        channels: number,
        startTime: number,
        duration: number,
      ) => void;
      stop: () => void;
      getPendingSamples: () => Promise<number>;
    };
    main: {
      getPluginNames: () => Promise<PluginNameInfo>;
      getFontsList: () => Promise<FontsList>;
      getConfig: () => Promise<AperioConfig>;
      saveConfig: (config: Partial<AperioConfig>) => Promise<void>;
      openContextMenu: (id: string) => Promise<void>;
      onAddObject: (
        cb: (objName: string, type: "Audio" | "Video") => void,
      ) => () => void;
      getEventStack: () => Promise<number>;
      onEventStackChanged: (cb: (length: number) => void) => () => void;
      resizeOsr: (width: number, height: number) => Promise<void>;
      showOpenDialog: (
        options: OpenDialogOptions,
      ) => Promise<string[] | undefined>;
    };
    // プラグインイベント
    events: EventApi;
  }
}
