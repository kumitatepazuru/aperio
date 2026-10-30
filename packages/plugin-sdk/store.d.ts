// ホストの zustand store (@aperio/ui/store) のダミー型定義。
import type {
  ColorPickerState,
  FrameState,
  ItemResult,
  ItemStructure,
  SyncableState,
} from "native";

export type Store = SyncableState & {
  frameResults: Record<string, ItemResult>;
  play: (beginFrame: number) => void;
  pause: (beginFrame?: number) => void;
  setFrameState: (frameState: FrameState) => void;
  setFrameResults: (frameResults: Record<string, ItemResult>) => void;
  setFrameCount: (frame: number) => void;
  setTimelineItems: (items: ItemStructure[]) => void;
  setSelectedItemId: (id: string | null) => void;
  addSelectedItemId: (id: string) => void;
  setSelectedItems: (ids: string[], mainId: string | null) => void;
  setColorPicker: (colorPicker: ColorPickerState) => void;
};

export interface UseStore {
  (): Store;
  <T>(selector: (state: Store) => T): T;
}

export declare const useStore: UseStore;
export default useStore;

export declare function getStoreState(): Promise<Store>;
