import useStore, { type TimelineLayerStructure } from "@shared/store";
import { useShallow } from "zustand/shallow";
import {
  useEffect,
  useRef,
  useState,
  type MouseEvent,
} from "react";

const GraduationSnapThresholdPx = 8;

type DragMode = "move" | "resize-start" | "resize-end";

type DragState = {
  /** ドラッグ開始したアイテムのID（スナップ基準） */
  anchorId: string;
  mode: DragMode;
  startClientX: number;
  startClientY: number;
  /** 移動対象アイテムのドラッグ開始時スナップショット */
  initItems: Array<{ id: string; from: number; to: number; layer: number }>;
};

type LayerItemsProps = {
  zoomLevelPxPerFrame: number;
  graduationInterval: number;
  layerHeight: number;
};

const snapFrame = (
  rawFrame: number,
  graduationInterval: number,
  zoomLevelPxPerFrame: number,
) => {
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
 * 移動アイテム同士がフレームで重なる場合、layer clamp で同じレイヤーに
 * 着地しないよう desiredLayerDelta を 0 方向へ制限して返す。
 */
function clampLayerDelta(
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
 * - 全選択アイテムに同じデルタを適用することで相対位置を保持する。
 * - 各アイテムの障害物から候補デルタを生成し、全アイテムに対して有効な
 *   desiredDelta に最も近い値を返す。
 * - どこにも置けない場合は 0（ドラッグ開始位置）を返す。
 */
function resolveGroupMoveDelta(
  allLayers: TimelineLayerStructure[],
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
      for (const o of allLayers) {
        if (movingIds.has(o.id) || o.layer !== targetLayer) continue;
        if (newFrom < o.to && newTo > o.from) return false;
      }
    }
    return true;
  };

  if (isValidDelta(desiredDelta)) return desiredDelta;

  // 各アイテム × 各障害物から候補デルタを収集（障害物の直前・直後に置くデルタ）
  // minDelta を含めることでフレーム0境界まで移動できるようにする
  const candidates = new Set<number>([0, minDelta]); // 0 = ドラッグ開始位置、minDelta = frame 0境界
  for (const init of initItems) {
    const targetLayer = Math.max(0, init.layer + layerDelta);
    const duration = init.to - init.from;
    for (const o of allLayers) {
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

const LayerItems = ({
  zoomLevelPxPerFrame,
  graduationInterval,
  layerHeight,
}: LayerItemsProps) => {
  const {
    timelineLayers,
    setTimelineLayers,
    selectedItemId,
    setSelectedItemId,
    addSelectedItemId,
  } = useStore(
    useShallow((state) => ({
      timelineLayers: state.timelineLayers,
      setTimelineLayers: state.setTimelineLayers,
      selectedItemId: state.selectedItemId,
      setSelectedItemId: state.setSelectedItemId,
      addSelectedItemId: state.addSelectedItemId,
    })),
  );
  const dragStateRef = useRef<DragState | null>(null);
  const [isDragging, setIsDragging] = useState(false);

  const beginItemDrag = (
    event: MouseEvent<HTMLDivElement>,
    anchorId: string,
    mode: DragMode,
    movingItemIds: string[],
    allLayers: TimelineLayerStructure[],
  ) => {
    event.preventDefault();
    event.stopPropagation();
    dragStateRef.current = {
      anchorId,
      mode,
      startClientX: event.clientX,
      startClientY: event.clientY,
      initItems: allLayers
        .filter((l) => movingItemIds.includes(l.id))
        .map((l) => ({ id: l.id, from: l.from, to: l.to, layer: l.layer })),
    };
  };

  useEffect(() => {
    const handleMouseMove = (event: globalThis.MouseEvent) => {
      const dragState = dragStateRef.current;
      if (!dragState) return;

      const frameDeltaRaw =
        (event.clientX - dragState.startClientX) / zoomLevelPxPerFrame;
      const layerDelta = Math.round(
        (event.clientY - dragState.startClientY) / layerHeight,
      );

      const anchorInit = dragState.initItems.find(
        (i) => i.id === dragState.anchorId,
      );
      if (!anchorInit) return;

      const movingIds = new Set(dragState.initItems.map((i) => i.id));

      if (dragState.mode === "move") {
        // アンカーアイテムの desiredFrom をスナップしてデルタを計算
        const desiredDelta =
          snapFrame(
            anchorInit.from + frameDeltaRaw,
            graduationInterval,
            zoomLevelPxPerFrame,
          ) - anchorInit.from;

        // アイテム同士が同じレイヤーに着地しないよう layerDelta を制限
        const effectiveLayerDelta = clampLayerDelta(dragState.initItems, layerDelta);

        // グループ全体で有効なデルタを解決（相対位置保持 + 衝突解決）
        const resolvedDelta = resolveGroupMoveDelta(
          timelineLayers,
          movingIds,
          dragState.initItems,
          effectiveLayerDelta,
          desiredDelta,
        );

        setTimelineLayers(
          timelineLayers.map((layer) => {
            const init = dragState.initItems.find((i) => i.id === layer.id);
            if (!init) return layer;
            const duration = init.to - init.from;
            const newFrom = Math.max(0, init.from + resolvedDelta);
            const targetLayerNum = Math.max(0, init.layer + effectiveLayerDelta);
            return {
              ...layer,
              from: newFrom,
              to: newFrom + duration,
              layer: targetLayerNum,
            };
          }),
        );
        return;
      }

      // resize操作（anchorId 単体のみ）
      setTimelineLayers(
        timelineLayers.map((layer) => {
          if (layer.id !== dragState.anchorId) return layer;
          const init = anchorInit;

          if (dragState.mode === "resize-start") {
            const nextFrom = snapFrame(
              init.from + frameDeltaRaw,
              graduationInterval,
              zoomLevelPxPerFrame,
            );
            const desiredFrom = Math.max(0, Math.min(nextFrom, init.to - 1));

            // 左側障害物（init.toより前に終わるもの）が左方向への拡張を制限する
            const minFrom = timelineLayers
              .filter(
                (o) =>
                  o.layer === layer.layer &&
                  !movingIds.has(o.id) &&
                  o.to <= init.from,
              )
              .reduce((acc, o) => Math.max(acc, o.to), 0);

            return { ...layer, from: Math.max(desiredFrom, minFrom) };
          }

          // resize-end
          const nextTo = snapFrame(
            init.to + frameDeltaRaw,
            graduationInterval,
            zoomLevelPxPerFrame,
          );
          const desiredTo = Math.max(init.from + 1, nextTo);

          // 右側障害物（init.to以降から始まるもの）が右方向への拡張を制限する
          const maxTo = timelineLayers
            .filter(
              (o) =>
                o.layer === layer.layer &&
                !movingIds.has(o.id) &&
                o.from >= init.to,
            )
            .reduce((acc, o) => Math.min(acc, o.from), Infinity);

          return { ...layer, to: Math.min(desiredTo, maxTo) };
        }),
      );
    };

    const handleMouseUp = () => {
      dragStateRef.current = null;
      setIsDragging(false);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);

    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [
    graduationInterval,
    layerHeight,
    setTimelineLayers,
    timelineLayers,
    zoomLevelPxPerFrame,
  ]);

  return timelineLayers.map((layer) => {
    const left = layer.from * zoomLevelPxPerFrame;
    const width = (layer.to - layer.from) * zoomLevelPxPerFrame;
    const top = layer.layer * layerHeight;
    const isSelected = selectedItemId.includes(layer.id);
    const isActiveDrag = isDragging && isSelected;

    return (
      <div
        key={layer.id}
        className="absolute py-0.5"
        style={{
          left,
          width,
          top,
          height: layerHeight,
        }}
      >
        <div
          className={`w-full h-full border-dotted border-base-100 rounded relative flex items-center ${
            isActiveDrag
              ? "bg-primary/70 cursor-grabbing"
              : "bg-primary cursor-grab"
          }`}
          onMouseDown={(event) => {
            if (event.shiftKey) {
              addSelectedItemId(layer.id);
            } else if (isSelected) {
              // 複数選択中のアイテムをドラッグ → 選択維持のまま全体を移動
              setIsDragging(true);
              beginItemDrag(
                event,
                layer.id,
                "move",
                selectedItemId,
                timelineLayers,
              );
            } else {
              setSelectedItemId(layer.id);
              setIsDragging(true);
              beginItemDrag(
                event,
                layer.id,
                "move",
                [layer.id],
                timelineLayers,
              );
            }
          }}
          style={{
            borderWidth: isSelected ? 2 : 0,
          }}
        >
          <div
            className="absolute left-0 top-0 bottom-0 w-2 cursor-ew-resize"
            onMouseDown={(event) => {
              setIsDragging(true);
              beginItemDrag(
                event,
                layer.id,
                "resize-start",
                [layer.id],
                timelineLayers,
              );
            }}
          />
          <div
            className="absolute right-0 top-0 bottom-0 w-2 cursor-ew-resize"
            onMouseDown={(event) => {
              setIsDragging(true);
              beginItemDrag(
                event,
                layer.id,
                "resize-end",
                [layer.id],
                timelineLayers,
              );
            }}
          />
          <span className="text-primary-content p-2 text-xs">
            {layer.obj.displayName}
          </span>
        </div>
      </div>
    );
  });
};

export default LayerItems;
