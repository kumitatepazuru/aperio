import { webContents } from "electron";

/** ハートビートが来なくなってから死亡判定するまでのms */
const HEARTBEAT_TIMEOUT_MS = 6000;
/** 死亡チェックの実行間隔ms */
const HEARTBEAT_CHECK_INTERVAL_MS = 3000;

type ClientEntry = {
  webContentsId: number;
  lastHeartbeat: number;
  alive: boolean;
};

export type RegisterResult = {
  clientId: number;
  /** 登録時点で生存している最小IDのクライアント。自分以外。いなければnull */
  masterId: number | null;
  masterWebContentsId: number | null;
};

export type HeartbeatResult = {
  /** 欠番IDでハートビートが来た場合は新IDを返す */
  clientId: number;
};

export type MasterResult = {
  masterId: number | null;
  masterWebContentsId: number | null;
};

export class RendezvousServer {
  private clients = new Map<number, ClientEntry>();
  private nextId = 1;
  private checkTimer: ReturnType<typeof setInterval> | null = null;

  start(): void {
    this.checkTimer = setInterval(
      () => this.checkHeartbeats(),
      HEARTBEAT_CHECK_INTERVAL_MS,
    );
  }

  stop(): void {
    if (this.checkTimer !== null) {
      clearInterval(this.checkTimer);
      this.checkTimer = null;
    }
  }

  register(requesterWebContentsId: number): RegisterResult {
    const clientId = this.nextId++;
    this.clients.set(clientId, {
      webContentsId: requesterWebContentsId,
      lastHeartbeat: Date.now(),
      alive: true,
    });

    const master = this.getMasterExcluding(clientId);
    return {
      clientId,
      masterId: master?.clientId ?? null,
      masterWebContentsId: master?.webContentsId ?? null,
    };
  }

  heartbeat(clientId: number, requesterWebContentsId: number): HeartbeatResult {
    const entry = this.clients.get(clientId);

    // 欠番IDまたはdead状態のハートビート → 新IDで再登録
    if (!entry || !entry.alive) {
      const newId = this.nextId++;
      this.clients.set(newId, {
        webContentsId: requesterWebContentsId,
        lastHeartbeat: Date.now(),
        alive: true,
      });
      return { clientId: newId };
    }

    entry.lastHeartbeat = Date.now();
    return { clientId };
  }

  /** 現在生存中の最小IDクライアントを返す。excludeClientIdは除外 */
  getMasterExcluding(
    excludeClientId?: number,
  ): { clientId: number; webContentsId: number } | null {
    let masterId: number | null = null;
    let masterEntry: ClientEntry | null = null;

    for (const [id, entry] of this.clients) {
      if (!entry.alive) continue;
      if (id === excludeClientId) continue;
      if (masterId === null || id < masterId) {
        masterId = id;
        masterEntry = entry;
      }
    }

    if (masterId === null || masterEntry === null) return null;
    return { clientId: masterId, webContentsId: masterEntry.webContentsId };
  }

  private checkHeartbeats(): void {
    const now = Date.now();
    for (const [clientId, entry] of this.clients) {
      if (!entry.alive) continue;
      if (now - entry.lastHeartbeat > HEARTBEAT_TIMEOUT_MS) {
        entry.alive = false;
        this.notifyClientDied(clientId);
      }
    }
  }

  private notifyClientDied(deadClientId: number): void {
    for (const [, entry] of this.clients) {
      if (!entry.alive) continue;
      const wc = webContents.fromId(entry.webContentsId);
      if (wc && !wc.isDestroyed()) {
        wc.send("rendezvous:client-died", deadClientId);
      }
    }
  }
}
