import {
  type FrameLayerStructure,
  type AperioConfig,
  type RequestStructureParameter,
  type NewGeneratorReturn,
} from "native";
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
        frameStruct: FrameLayerStructure[],
      ) => Promise<void>;
      setReceiver: (
        cb: Parameters<typeof sharedTexture.setSharedTextureReceiver>[0],
      ) => void;
      getFrameSharedTexture: (
        count: number,
        frameStruct: FrameLayerStructure[],
      ) => Promise<void>;
    };
    main: {
      getPluginNames: () => Record<string, string>[];
      requestNewGenerator: (
        pluginName: string,
        args: Record<string, unknown>,
      ) => Promise<NewGeneratorReturn>;
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
