import type { LayerStructure } from "native";
import { create } from "zustand";
import type { ColorValue } from "@/configable/utils";

export type TimelineLayerStructure = LayerStructure & {
  id: string; // UUIDが期待される
  layer: number; // レイヤー番号
  from: number; // 開始フレーム
  to: number; // 終了フレーム
};

type ViewerState = {
  state: "playing" | "paused";
  changeTime: number; // 状態変更時刻のタイムスタンプ Date.now()基準（クロスrenderer同期のため）
  beginFrame: number; // 再生開始フレーム
};

export type ColorSpace = "HSV" | "LCH" | "okLCH" | "LAB" | "okLAB";
export type DisplayMode = "0-1" | "0-255";

type Store = {
  fps: number;
  frameState: {
    width: number;
    height: number;
  };
  viewerState: ViewerState;
  timelineLayers: TimelineLayerStructure[];
  selectedItemId: string | null;
  colorPicker: {
    colorSpace: ColorSpace;
    displayMode: DisplayMode;
    history: ColorValue[];
  };
  play: (beginFrame: number) => void;
  pause: (beginFrame?: number) => void;
  setFrameState: (frameState: { width: number; height: number }) => void;
  setFrameCount: (frame: number) => void;
  setFps: (fps: number) => void;
  setTimelineLayers: (layers: TimelineLayerStructure[]) => void;
  setSelectedItemId: (id: string | null) => void;
  setColorPicker: (colorPicker: Store["colorPicker"]) => void;
};

// 同期対象の直列化可能なstateのみ
export type SyncableState = Pick<
  Store,
  | "fps"
  | "viewerState"
  | "timelineLayers"
  | "frameState"
  | "selectedItemId"
  | "colorPicker"
>;

// ─── BroadcastChannel ────────────────────────────────────────────────────────

type ChannelMessage =
  | { type: "state"; data: Partial<SyncableState> }
  /** 新クライアントが初回同期を開始。全クライアントのget/setをブロック */
  | { type: "sync-lock" }
  /** 初回同期完了。全クライアントのキューを drain してブロック解除 */
  | { type: "sync-unlock" };

const channel = new BroadcastChannel("aperio-store-sync");

// ─── グローバルロック ─────────────────────────────────────────────────────────
// 初回同期中は全クライアントの get/set をブロックする。
//
// get のブロック方法:
//   - useStore (React hook):       syncReadyPromise を throw → React Suspense が待機
//   - useStore.getState() / getStoreState(): 同上。呼び出し元は catch して await するか
//                                   Suspense に委譲する。
//   - getCurrentFrameCount / getFrameStruct: async で syncReadyPromise を await してから
//                                   _useStore.getState() を直接参照する。
//
// set のブロック方法:
//   - setQueue に積み、drain 時に順番に適用する。
//
// drainQueue → Promise resolve の順を守ることで、resolve 後の再レンダリング・
// await 再開時には最新 state が揃っている。

let globalLockCount = 0;
/** ロック中のみ存在。解除時に resolve される。 */
let syncReadyPromise: Promise<void> | null = null;
let syncReadyResolve: (() => void) | null = null;
const setQueue: Array<() => void> = [];

function incrementLock() {
  if (globalLockCount === 0) {
    syncReadyPromise = new Promise<void>((resolve) => {
      syncReadyResolve = resolve;
    });
  }
  globalLockCount++;
}

function decrementLock() {
  globalLockCount--;
  if (globalLockCount === 0) {
    drainQueue();
    const resolve = syncReadyResolve;
    syncReadyPromise = null;
    syncReadyResolve = null;
    resolve?.();
  }
}

function drainQueue() {
  const items = setQueue.splice(0);
  for (const fn of items) fn();
}

/** 自分がロックを取得し、他クライアントにもロックを通知する */
function acquireSyncLock() {
  incrementLock();
  channel.postMessage({ type: "sync-lock" } satisfies ChannelMessage);
}

/** ロックを解放し、他クライアントにも解放を通知。キューをdrainしてPromiseを解決する */
function releaseSyncLock() {
  channel.postMessage({ type: "sync-unlock" } satisfies ChannelMessage);
  decrementLock();
}

// ─── Internal Store ──────────────────────────────────────────────────────────

const _useStore = create<Store>()((set, get) => {
  channel.onmessage = (e: MessageEvent<ChannelMessage>) => {
    const msg = e.data;
    if (msg.type === "state") {
      const apply = () => set(msg.data);
      if (globalLockCount > 0) {
        setQueue.push(apply);
      } else {
        apply();
      }
    } else if (msg.type === "sync-lock") {
      incrementLock();
    } else if (msg.type === "sync-unlock") {
      decrementLock();
    }
  };

  // setしつつ他のrendererへ伝播する。ロック中はキューイング。
  const syncSet = (partial: Partial<SyncableState>) => {
    const apply = () => {
      set(partial);
      channel.postMessage({
        type: "state",
        data: partial,
      } satisfies ChannelMessage);
    };
    if (globalLockCount > 0) {
      setQueue.push(apply);
    } else {
      apply();
    }
  };

  return {
    fps: 60,
    frameState: {
      width: 1920,
      height: 1080,
    },
    viewerState: {
      state: "paused",
      changeTime: Date.now(),
      beginFrame: 0,
    },
    timelineLayers: [],
    pluginNames: undefined,
    selectedItemId: null,
    colorPicker: {
      colorSpace: "HSV",
      displayMode: "0-255",
      history: [],
    },
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
    setFrameState: (frameState: { width: number; height: number }) =>
      syncSet({ frameState }),
    setFrameCount: (frame) =>
      syncSet({
        viewerState: {
          state: "paused",
          changeTime: Date.now(),
          beginFrame: frame,
        },
      }),
    setFps: (fps) => syncSet({ fps }),
    setTimelineLayers: (layers) => syncSet({ timelineLayers: layers }),
    setSelectedItemId: (id: string | null) => syncSet({ selectedItemId: id }),
    setColorPicker: (colorPicker: Store["colorPicker"]) =>
      syncSet({ colorPicker }),
  };
});

// ─── Public API ──────────────────────────────────────────────────────────────

/**
 * Suspense-aware な zustand hook（default export）。
 * - useStore() / useStore(selector): ロック中は syncReadyPromise を throw → Suspense 待機
 * - useStore.getState(): バグリスクが高いため無効化。必要なら waitForStoreState() を使うこと。
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
  // 直接呼び出すとバグのもとになるため、型レベルで無効化。
  // vscodeのtsプラグインだとこれでエラーになるがtsに定義された動作ではないためかなりハック的。
  // TODO: 解決方法がわかり次第より安定した無効化実装に書き換え
  getState: () => void;
} & typeof _useStore;

/**
 * ロック中は syncReadyPromise を await してから state を取得する async ユーティリティ。
 * React 外で「ロックが解けるまで待ってから処理したい」ケースに使う。
 */
export async function getStoreState(): Promise<Store> {
  if (syncReadyPromise) await syncReadyPromise;
  return _useStore.getState();
}

export const getCurrentFrameCount = async (): Promise<number> => {
  const { viewerState, fps } = await getStoreState();
  if (viewerState.state === "playing") {
    const elapsedTime = (Date.now() - viewerState.changeTime) / 1000;
    return viewerState.beginFrame + Math.floor(elapsedTime * fps);
  }
  return viewerState.beginFrame;
};

export const getFrameStruct = async () => {
  const state = await getStoreState();
  const { viewerState, fps } = state;
  const currentFrame =
    viewerState.state === "playing"
      ? viewerState.beginFrame +
        Math.floor(((Date.now() - viewerState.changeTime) / 1000) * fps)
      : viewerState.beginFrame;
  return state.timelineLayers
    .filter((layer) => currentFrame >= layer.from && currentFrame <= layer.to)
    .sort((a, b) => a.layer - b.layer);
};

// ─── Rendezvous 初回同期 ─────────────────────────────────────────────────────

let myClientId: number | null = null;
/** 現在同期中の master clientId。死亡通知の照合に使う */
let syncTargetClientId: number | null = null;
/** master 死亡時に requestState の待機を即時中断するためのシグナル */
let masterDeathSignal: (() => void) | null = null;

function getSyncableState(): SyncableState {
  // 内部呼び出し（provideState ハンドラ）なのでロックを bypass して直接取得する
  const s = _useStore.getState();
  return {
    fps: s.fps,
    viewerState: s.viewerState,
    timelineLayers: s.timelineLayers,
    frameState: s.frameState,
    selectedItemId: s.selectedItemId,
    colorPicker: s.colorPicker,
  };
}

/**
 * ランデブーサーバーに master を問い合わせて state を1回取得試行する。
 * - 成功（state 取得）→ true
 * - master なし or 自分が master → true（同期不要）
 * - master 死亡通知 → false（呼び出し元がリトライ）
 */
async function trySyncOnce(): Promise<boolean> {
  const master = await window.rendezvous.getMaster();
  if (!master.masterId || !master.masterWebContentsId) {
    console.log("No master found. This client will be the master.");
    return true;
  }
  if (master.masterId === myClientId) {
    console.log("This client is the master. No need to sync.");
    return true;
  }

  syncTargetClientId = master.masterId;

  // master 死亡通知で即時 resolve できるよう Promise を用意
  const deathPromise = new Promise<null>((resolve) => {
    masterDeathSignal = () => resolve(null);
  });

  const state = await Promise.race([
    window.rendezvous.requestState(master.masterWebContentsId),
    deathPromise,
  ]);

  masterDeathSignal = null;
  syncTargetClientId = null;

  if (state) {
    // ロックを保持したまま直接 set（syncSet を経由しない＝再キューイングしない）
    console.log("State received from master. Sync complete.");
    _useStore.setState(state);
    return true;
  }
  console.log("Failed to get state from master. Will retry.");
  return false; // 死亡 or タイムアウト → リトライ
}

async function initRendezvousSync(): Promise<void> {
  // state 要求が来たら現在の state を返す（自分が master 候補のとき）
  window.rendezvous.onProvideState((requesterId) => {
    console.log(
      `State requested by client ${requesterId}. Providing current state.`,
    );
    void window.rendezvous.stateResponse(requesterId, getSyncableState());
  });

  // master 死亡通知: 同期待機中なら即時中断して再試行させる
  window.rendezvous.onClientDied((deadClientId) => {
    console.log(
      `Client ${deadClientId} died. Checking if it was the sync target.`,
    );
    if (syncTargetClientId === deadClientId && masterDeathSignal) {
      masterDeathSignal();
    }
  });

  // ロックをすぐに取得: register 完了前の get/set も含めてすべてブロック
  acquireSyncLock();

  try {
    const { clientId, masterId } = await window.rendezvous.register();
    myClientId = clientId;

    // ハートビート送信（2秒ごと）
    setInterval(() => {
      if (myClientId === null) return;
      void window.rendezvous.heartbeat(myClientId).then((result) => {
        if (result.clientId !== myClientId) {
          myClientId = result.clientId;
        }
      });
    }, 1000);

    if (masterId !== null) {
      // master がいる間はリトライし続ける
      let done = false;
      while (!done) {
        done = await trySyncOnce();
      }
    }
  } finally {
    // 成功・失敗どちらでも必ずロック解放 → drain → Promise resolve → get/set 再開
    releaseSyncLock();
  }
}

void initRendezvousSync();

// ─────────────────────────────────────────────────────────────────────────────

export default useStore;
