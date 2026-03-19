import { v4 as uuidv4 } from "uuid";
import type { LayerStructure } from "native";
import { create } from "zustand";

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

type Store = {
  fps: number;
  frameState: {
    width: number;
    height: number;
  };
  viewerState: ViewerState;
  play: (beginFrame: number) => void;
  pause: (beginFrame?: number) => void;
  setFrameState: (frameState: { width: number; height: number }) => void;
  getPluginNames: () => { [key: string]: string }[];
  setFrameCount: (frame: number) => void;
  setFps: (fps: number) => void;
  timelineLayers: TimelineLayerStructure[];
  setTimelineLayers: (layers: TimelineLayerStructure[]) => void;
  pluginNames?: { [key: string]: string }[];
};

// 同期対象の直列化可能なstateのみ
export type SyncableState = Pick<
  Store,
  "fps" | "viewerState" | "timelineLayers" | "pluginNames" | "frameState"
>;

// ─── BroadcastChannel ────────────────────────────────────────────────────────

type ChannelMessage =
  | { type: "state"; data: Partial<SyncableState> }
  /** 新クライアントが初回同期を開始。全クライアントのsetをキューイング */
  | { type: "sync-lock" }
  /** 初回同期完了。全クライアントのキューを drain */
  | { type: "sync-unlock" };

const channel = new BroadcastChannel("aperio-store-sync");

// ─── グローバルロック ─────────────────────────────────────────────────────────
// 初回同期中は全クライアントのsetをキューイングする。
// BroadcastChannelはsenderには届かないためself側は acquireSyncLock/releaseSyncLock で直接操作する。

let globalLockCount = 0;
const setQueue: Array<() => void> = [];

function drainQueue() {
  const items = setQueue.splice(0);
  for (const fn of items) fn();
}

/** 自分がロックを取得し、他クライアントにもロックを通知する */
function acquireSyncLock() {
  globalLockCount++;
  channel.postMessage({ type: "sync-lock" } satisfies ChannelMessage);
}

/** ロックを解放し、他クライアントにも解放を通知。自分のキューもdrainする */
function releaseSyncLock() {
  channel.postMessage({ type: "sync-unlock" } satisfies ChannelMessage);
  globalLockCount--;
  if (globalLockCount === 0) drainQueue();
}

// ─── Store ───────────────────────────────────────────────────────────────────

const useStore = create<Store>()((set, get) => {
  channel.onmessage = (e: MessageEvent<ChannelMessage>) => {
    const msg = e.data;
    if (msg.type === "state") {
      // ロック中は他クライアントからのsetもキューイング
      const apply = () => set(msg.data);
      if (globalLockCount > 0) {
        setQueue.push(apply);
      } else {
        apply();
      }
    } else if (msg.type === "sync-lock") {
      globalLockCount++;
    } else if (msg.type === "sync-unlock") {
      globalLockCount--;
      if (globalLockCount === 0) drainQueue();
    }
  };

  // setしつつ他のrendererへ伝播する。ロック中はキューイング。
  const syncSet = (partial: Partial<SyncableState>) => {
    const apply = () => {
      set(partial);
      channel.postMessage({ type: "state", data: partial } satisfies ChannelMessage);
    };
    if (globalLockCount > 0) {
      setQueue.push(apply);
    } else {
      apply();
    }
  };

  return {
    // get関数は呼び出し元rendererのローカルで実行される
    fps: 60,
    frameState: {
      width: 3840,
      height: 2160,
    },
    viewerState: {
      state: "paused",
      changeTime: Date.now(),
      beginFrame: 0,
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
    timelineLayers: [
      {
        id: uuidv4(),
        layer: 0,
        from: 0,
        to: 1000,
        x: 500,
        y: 500,
        scale: 3.0,
        rotation: 40.0,
        alpha: 1.0,
        obj: {
          name: "TestObject",
          parameters: {},
        },
        effects: [],
      },
    ],
    setTimelineLayers: (layers) => syncSet({ timelineLayers: layers }),
    pluginNames: undefined,
    getPluginNames: () => {
      let names = get().pluginNames;
      if (!names) {
        names = window.main.getPluginNames();
        syncSet({ pluginNames: names });
      }
      return names;
    },
  };
});

export const getCurrentFrameCount = () => {
  const { viewerState, fps } = useStore.getState();
  if (viewerState.state === "playing") {
    const elapsedTime = (Date.now() - viewerState.changeTime) / 1000;
    return viewerState.beginFrame + Math.floor(elapsedTime * fps);
  } else {
    return viewerState.beginFrame;
  }
};

export const getFrameStruct = () => {
  const currentFrame = getCurrentFrameCount();
  return useStore
    .getState()
    .timelineLayers.filter((layer) => {
      return currentFrame >= layer.from && currentFrame <= layer.to;
    })
    .sort((a, b) => a.layer - b.layer);
};

// ─── Rendezvous 初回同期 ─────────────────────────────────────────────────────

let myClientId: number | null = null;
/** 現在同期中の master clientId。死亡通知の照合に使う */
let syncTargetClientId: number | null = null;
/** master 死亡時に requestState の待機を即時中断するためのシグナル */
let masterDeathSignal: (() => void) | null = null;

function getSyncableState(): SyncableState {
  const s = useStore.getState();
  return {
    fps: s.fps,
    viewerState: s.viewerState,
    timelineLayers: s.timelineLayers,
    pluginNames: s.pluginNames,
    frameState: s.frameState,
  };
}

/**
 * ランデブーサーバーに master を問い合わせて state を1回取得試行する。
 * - 成功（state 取得）→ true
 * - master なし or 自分が master → true（同期不要）
 * - タイムアウト or master 死亡通知 → false（呼び出し元がリトライ）
 */
async function trySyncOnce(): Promise<boolean> {
  const master = await window.rendezvous.getMaster();
  if (!master.masterId || !master.masterWebContentsId) {
    // master なし → 自分が master 扱いで同期不要
    console.log("No master found. This client will be the master.");
    return true;
  }
  if (master.masterId === myClientId) {
    // 自分が master → 同期不要
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
    useStore.setState(state);
    return true;
  }
  console.log("Failed to get state from master. Will retry.");
  return false; // 死亡 or タイムアウト → リトライ
}

async function initRendezvousSync(): Promise<void> {
  // state 要求が来たら現在の state を返す（自分が master 候補のとき）
  window.rendezvous.onProvideState((requesterId) => {
    console.log(`State requested by client ${requesterId}. Providing current state.`);
    window.rendezvous.stateResponse(requesterId, getSyncableState());
  });

  // master 死亡通知: 同期待機中なら即時中断して再試行させる
  window.rendezvous.onClientDied((deadClientId) => {
    console.log(`Client ${deadClientId} died. Checking if it was the sync target.`);
    if (syncTargetClientId === deadClientId && masterDeathSignal) {
      masterDeathSignal();
    }
  });

  // ロックをすぐに取得: register 完了前の set も含めてすべてキューイング
  acquireSyncLock();

  try {
    const { clientId, masterId } = await window.rendezvous.register();
    myClientId = clientId;

    // ハートビート送信（2秒ごと）
    setInterval(() => {
      if (myClientId === null) return;
      window.rendezvous.heartbeat(myClientId).then((result) => {
        if (result.clientId !== myClientId) {
          myClientId = result.clientId;
        }
      });
    }, 2000);

    if (masterId !== null) {
      // master がいる間はリトライし続ける
      let done = false;
      while (!done) {
        done = await trySyncOnce();
      }
    }
  } finally {
    // 成功・失敗どちらでも必ずロック解放 → キュードrainで通常動作再開
    releaseSyncLock();
  }
}

initRendezvousSync();

// ─────────────────────────────────────────────────────────────────────────────

export default useStore;
