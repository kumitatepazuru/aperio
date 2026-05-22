import type { ItemStructure } from "native";

const GraduationSnapThresholdPx = 8;

/**
 * rawFrame を目盛り（graduation）にスナップする。
 * 目盛りとの距離が GraduationSnapThresholdPx 以下であれば目盛り値を返し、
 * そうでなければ最近傍の整数フレームを返す。
 *
 * @param rawFrame - ピクセルデルタから算出した生のフレーム数（小数あり）
 * @param graduationInterval - 目盛り間隔（フレーム数）
 * @param zoomLevelPxPerFrame - 1フレームあたりのピクセル数（ズームレベル）
 * @returns スナップ後のフレーム数（整数）
 */
export const snapFrame = (
  rawFrame: number,
  graduationInterval: number,
  zoomLevelPxPerFrame: number,
): number => {
  const graduationFrame =
    Math.round(rawFrame / graduationInterval) * graduationInterval;
  const graduationDistancePx =
    Math.abs(rawFrame - graduationFrame) * zoomLevelPxPerFrame;
  if (graduationDistancePx <= GraduationSnapThresholdPx) {
    return graduationFrame;
  }
  return Math.round(rawFrame);
};

/**
 * 複数アイテムを同じデルタ分レイヤー移動させたとき、移動グループ内のアイテム同士が
 * 同じレイヤーに重なって衝突しないよう desiredLayerDelta を 0 方向へ制限して返す。
 * desiredLayerDelta が 0 の場合はそのまま 0 を返す。
 *
 * @param initItems - 移動対象アイテムのドラッグ開始時スナップショット（id / from / to / layer）
 * @param desiredLayerDelta - ユーザー操作により要求されたレイヤー移動量
 * @returns 移動グループ内で衝突しない最大の layerDelta。
 *          すべての非ゼロ値で衝突する場合は 0 を返す。
 */
export function clampLayerDelta(
  initItems: Array<{ id: string; from: number; to: number; layer: number }>,
  desiredLayerDelta: number,
): number {
  const hasConflict = (ld: number): boolean => {
    for (let i = 0; i < initItems.length; i++) {
      const a = initItems[i];
      const aLayer = Math.max(0, a.layer + ld);
      for (let j = i + 1; j < initItems.length; j++) {
        const b = initItems[j];
        if (Math.max(0, b.layer + ld) !== aLayer) continue;
        if (a.from < b.to && a.to > b.from) return true;
      }
    }
    return false;
  };

  if (!hasConflict(desiredLayerDelta)) return desiredLayerDelta;

  const step = desiredLayerDelta > 0 ? -1 : 1;
  for (let ld = desiredLayerDelta + step; ld !== 0; ld += step) {
    if (!hasConflict(ld)) return ld;
  }
  return 0;
}

/**
 * グループ移動の frameDelta を衝突解決しながら返す。
 *
 * 全選択アイテムに同じデルタを適用することで相対位置を保持しつつ、
 * 非選択アイテムとの衝突を回避する。desiredDelta が有効でない場合は、
 * 各障害物の直前・直後を候補デルタとして収集し、desiredDelta に最も近い
 * 有効値を返す。どこにも置けない場合は 0（ドラッグ開始位置）を返す。
 *
 * @param allItems - タイムライン上の全アイテム
 * @param movingIds - 移動中のアイテム ID セット（衝突判定から除外される）
 * @param initItems - 移動対象アイテムのドラッグ開始時スナップショット（id / from / to / layer）
 * @param layerDelta - 適用するレイヤー移動量（clampLayerDelta で制限済みの値を想定）
 * @param desiredDelta - ユーザー操作により要求されたフレーム移動量
 * @returns 衝突しない、desiredDelta に最も近いフレーム移動量。
 *          有効な位置が見つからない場合は 0 を返す。
 */
export function resolveGroupMoveDelta(
  allItems: ItemStructure[],
  movingIds: Set<string>,
  initItems: Array<{ id: string; from: number; to: number; layer: number }>,
  layerDelta: number,
  desiredDelta: number,
): number {
  if (initItems.length === 0) return desiredDelta;

  // いずれかのアイテムが frame 0 より前に行かないよう最小デルタを計算
  const minDelta = -Math.min(...initItems.map((i) => i.from));

  const isValidDelta = (D: number): boolean => {
    if (D < minDelta) return false;
    for (const init of initItems) {
      const targetLayer = Math.max(0, init.layer + layerDelta);
      const newFrom = init.from + D;
      const newTo = newFrom + (init.to - init.from);
      for (const o of allItems) {
        if (movingIds.has(o.id) || o.layer !== targetLayer) continue;
        if (newFrom < o.to && newTo > o.from) return false;
      }
    }
    return true;
  };

  if (isValidDelta(desiredDelta)) return desiredDelta;

  // 各アイテム × 各障害物から候補デルタを収集（障害物の直前・直後に置くデルタ）
  // minDelta を含めることでフレーム0境界まで移動できるようにする
  const candidates = new Set<number>([0, minDelta]);
  for (const init of initItems) {
    const targetLayer = Math.max(0, init.layer + layerDelta);
    const duration = init.to - init.from;
    for (const o of allItems) {
      if (movingIds.has(o.id) || o.layer !== targetLayer) continue;
      candidates.add(o.from - duration - init.from); // このアイテムを障害物の直前に
      candidates.add(o.to - init.from); // このアイテムを障害物の直後に
    }
  }

  let best: number | null = null;
  let bestDist = Infinity;
  for (const D of candidates) {
    if (!isValidDelta(D)) continue;
    const dist = Math.abs(D - desiredDelta);
    if (dist < bestDist) {
      bestDist = dist;
      best = D;
    }
  }

  return best ?? 0;
}
