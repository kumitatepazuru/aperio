import {
  type AperioConfig,
  type RequestStructureParameter,
  type NewEffectGeneratorReturn,
  type NewObjectGeneratorReturn,
  type PluginNameInfo,
  type ItemStructure,
} from "native";

/** フォントファミリー名 → ウェイト値（100/200/…/900）の配列 */
type FontsList = Record<string, number[]>;
import { sharedTexture } from "electron";
import type { SyncableState } from "../shared/store";

declare global {
  interface Window {
    rendezvous: {
      register: () => Promise<{
        clientId: number;
        masterId: number | null;
        masterWebContentsId: number | null;
      }>;
      heartbeat: (clientId: number) => Promise<{ clientId: number }>;
      getMaster: () => Promise<{
        masterId: number | null;
        masterWebContentsId: number | null;
      }>;
      requestState: (
        masterWebContentsId: number,
      ) => Promise<SyncableState | null>;
      stateResponse: (
        requesterId: number,
        state: SyncableState,
      ) => Promise<void>;
      onProvideState: (
        cb: (requesterWebContentsId: number) => void,
      ) => () => void;
      onClientDied: (cb: (deadClientId: number) => void) => () => void;
    };
    frame: {
      sendPort: () => Promise<void>;
      getFrameBuf: (
        count: number,
        width: number,
        height: number,
        fps: number,
        frameStruct: ItemStructure[],
      ) => Promise<void>;
      setReceiver: (
        cb: Parameters<typeof sharedTexture.setSharedTextureReceiver>[0],
      ) => void;
      getFrameSharedTexture: (
        count: number,
        fps: number,
        frameStruct: ItemStructure[],
      ) => Promise<void>;
    };
    main: {
      getPluginNames: () => Promise<PluginNameInfo>;
      getFontsList: () => Promise<FontsList>;
      requestNewObjectGenerator: (
        pluginName: string,
        args: Record<string, unknown>,
      ) => Promise<NewObjectGeneratorReturn>;
      requestNewEffectGenerator: (
        pluginName: string,
      ) => Promise<NewEffectGeneratorReturn>;
      requestParameterStruct: (
        pluginName: string,
        params: Record<string, unknown>,
      ) => Promise<RequestStructureParameter[]>;
      getConfig: () => Promise<AperioConfig>;
      saveConfig: (config: Partial<AperioConfig>) => Promise<void>;
      openContextMenu: (id: string) => Promise<void>;
      onAddObject: (cb: (objName: string) => void) => () => void;
      getEventStack: () => Promise<number>;
      onEventStackChanged: (cb: (length: number) => void) => () => void;
      resizeOsr: (width: number, height: number) => Promise<void>;
    };
  }
}
