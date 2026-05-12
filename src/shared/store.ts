import type {
  ItemResult,
  ItemStructure,
  SyncableState,
  SyncableStatePartial,
  FrameState,
  ColorPickerState,
} from "native";
import { create } from "zustand";

// ─── Store 型 ─────────────────────────────────────────────────────────────────

type Store = SyncableState & {
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

// ─── 初回同期フラグ ───────────────────────────────────────────────────────────
//
// syncing=true の間:
//   - useStore() は syncReadyPromise を throw → React Suspense が待機
//   - getStoreState() は syncReadyPromise を await
//   - 受信した diff は diffQueue に積む
// syncing=false になったら:
//   - diffQueue を drain してから syncReadyPromise を resolve
//   - 以降の diff は即時適用

let syncing = true;
let syncReadyResolve: (() => void) | null = null;
let syncReadyPromise: Promise<void> | null = new Promise<void>((resolve) => {
  syncReadyResolve = resolve;
});

const diffQueue: Array<Partial<Store>> = [];

// ─── 変換ユーティリティ ───────────────────────────────────────────────────────

// NAPI は Option<String> フィールドを None のとき undefined として返す。
// mainSelectedItemId は ts_type="string | null" だが runtime は undefined になり得るため null に正規化する。
const fromNative = (state: SyncableState): SyncableState => ({
  ...state,
  mainSelectedItemId: state.mainSelectedItemId ?? null,
});

// zustand 内部の Partial<Store> を native の SyncableStatePartial に変換する。
// mainSelectedItemId が null の場合は送信しない（selected_item_ids=[] による auto-clear に委ねる）。
const toNativePartial = (partial: Partial<SyncableState>): SyncableStatePartial => {
  const { mainSelectedItemId, ...rest } = partial;
  const result: SyncableStatePartial = { ...rest };
  if (mainSelectedItemId != null) {
    result.mainSelectedItemId = mainSelectedItemId;
  }
  return result;
};

// ─── Internal Store ──────────────────────────────────────────────────────────

const _useStore = create<Store>()((set, get) => {
  window.sync.onDiff((partial) => {
    const normalized: Partial<Store> = { ...partial };
    // Rust が selected_item_ids=[] で auto-clear するのと対称に、
    // diff を受け取った側も selectedItemIds=[] なら mainSelectedItemId を null にする。
    if (partial.selectedItemIds?.length === 0) {
      normalized.mainSelectedItemId = null;
    }
    if (syncing) {
      diffQueue.push(normalized);
    } else {
      set(normalized);
    }
  });

  const syncSet = (partial: Partial<SyncableState>) => {
    set(partial);
    window.sync.set(toNativePartial(partial));
  };

  // デフォルト値は Rust 側で定義済み。
  // ここは Suspense がブロックするため never rendered な placeholder。
  return {
    viewerState: { state: "paused", changeTime: 0, beginFrame: 0 },
    timelineItems: [],
    frameState: { width: 1920, height: 1080, fps: 60 },
    selectedItemIds: [],
    mainSelectedItemId: null,
    colorPicker: { colorSpace: "HSV", displayMode: "0-255", history: [] },
    frameResults: {},
    play: (beginFrame: number) =>
      syncSet({
        viewerState: {
          state: "playing",
          changeTime: Date.now(),
          beginFrame,
        },
      }),
    pause: (beginFrame?: number) =>
      syncSet({
        viewerState: {
          state: "paused",
          changeTime: Date.now(),
          beginFrame: beginFrame ?? get().viewerState.beginFrame,
        },
      }),
    setFrameState: (frameState: FrameState) => syncSet({ frameState }),
    setFrameResults: (frameResults) => set({ frameResults }),
    setFrameCount: (frame) =>
      syncSet({
        viewerState: {
          state: "paused",
          changeTime: Date.now(),
          beginFrame: frame,
        },
      }),
    setTimelineItems: (items) => syncSet({ timelineItems: items }),
    setSelectedItemId: (id: string | null) =>
      id !== null
        ? syncSet({ selectedItemIds: [id], mainSelectedItemId: id })
        : syncSet({ selectedItemIds: [] }),
    addSelectedItemId: (id: string) => {
      const current = get();
      syncSet({
        selectedItemIds: [...current.selectedItemIds, id],
        mainSelectedItemId: current.mainSelectedItemId ?? id,
      });
    },
    setSelectedItems: (ids: string[], mainId: string | null) =>
      mainId !== null
        ? syncSet({ selectedItemIds: ids, mainSelectedItemId: mainId })
        : syncSet({ selectedItemIds: ids }),
    setColorPicker: (colorPicker: ColorPickerState) => syncSet({ colorPicker }),
  };
});

// ─── Public API ──────────────────────────────────────────────────────────────

/**
 * Suspense-aware な zustand hook（default export）。
 * - useStore() / useStore(selector): 初回同期中は syncReadyPromise を throw → Suspense 待機
 * - useStore.getState(): 同期ずれが発生する可能性があるため無効化。必要なら getStoreState() を使うこと。
 * - useStore.setState() / .subscribe(): _useStore に直接委譲
 */
const useStore = Object.assign(
  (selector?: (state: Store) => unknown) => {
    if (syncReadyPromise !== null) throw syncReadyPromise;
    return selector !== undefined ? _useStore(selector) : _useStore();
  },
  {
    setState: _useStore.setState.bind(_useStore),
    subscribe: _useStore.subscribe.bind(_useStore),
  },
) as {
  // 直接呼び出すと同期ずれのもとになるため、型レベルで無効化。
  // vscodeのtsプラグインだとこれでエラーになるがtsに定義された動作ではないためかなりハック的。
  // TODO: 解決方法がわかり次第より安定した無効化実装に書き換え
  getState: () => void;
} & typeof _useStore;

/**
 * 初回同期中は syncReadyPromise を await してから state を取得する async ユーティリティ。
 */
export async function getStoreState(): Promise<Store> {
  if (syncReadyPromise) await syncReadyPromise;
  return _useStore.getState();
}

export const getCurrentFrameCount = async (): Promise<number> => {
  const { viewerState, frameState } = await getStoreState();
  if (viewerState.state === "playing") {
    const elapsedTime = (Date.now() - viewerState.changeTime) / 1000;
    return viewerState.beginFrame + Math.floor(elapsedTime * frameState.fps);
  }
  return viewerState.beginFrame;
};

export const getCurrentFrameStruct = async () => {
  const state = await getStoreState();
  const { viewerState, frameState } = state;
  const currentFrame =
    viewerState.state === "playing"
      ? viewerState.beginFrame +
        Math.floor(
          ((Date.now() - viewerState.changeTime) / 1000) * frameState.fps,
        )
      : viewerState.beginFrame;
  return state.timelineItems
    .filter((item) => currentFrame >= item.from && currentFrame <= item.to)
    .sort((a, b) => a.layer - b.layer);
};

// ─── 初回同期 ─────────────────────────────────────────────────────────────────

async function initSync(): Promise<void> {
  const nativeState = await window.sync.register();
  _useStore.setState(fromNative(nativeState));
  for (const diff of diffQueue.splice(0)) {
    _useStore.setState(diff);
  }
  syncing = false;
  const resolve = syncReadyResolve;
  syncReadyPromise = null;
  syncReadyResolve = null;
  resolve?.();
}

void initSync();

// ─────────────────────────────────────────────────────────────────────────────

export default useStore;
