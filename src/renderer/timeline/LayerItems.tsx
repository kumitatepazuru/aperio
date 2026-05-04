import useStore from "@shared/store";
import { useShallow } from "zustand/shallow";
import { useEffect, useRef, useState, type MouseEvent } from "react";
import { snapFrame, clampLayerDelta, resolveGroupMoveDelta } from "@shared/utils/layerUtils";
import type { ItemStructure } from "native";

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

const LayerItems = ({
  zoomLevelPxPerFrame,
  graduationInterval,
  layerHeight,
}: LayerItemsProps) => {
  const {
    timelineItems,
    setTimelineItems,
    selectedItemIds,
    setSelectedItemId,
    addSelectedItemId,
    setSelectedItems,
  } = useStore(
    useShallow((state) => ({
      timelineItems: state.timelineItems,
      setTimelineItems: state.setTimelineItems,
      selectedItemIds: state.selectedItemIds,
      setSelectedItemId: state.setSelectedItemId,
      addSelectedItemId: state.addSelectedItemId,
      setSelectedItems: state.setSelectedItems,
    })),
  );
  const dragStateRef = useRef<DragState | null>(null);
  const [isDragging, setIsDragging] = useState(false);

  const beginItemDrag = (
    event: MouseEvent<HTMLDivElement>,
    anchorId: string,
    mode: DragMode,
    movingItemIds: string[],
    allItems: ItemStructure[],
  ) => {
    event.preventDefault();
    event.stopPropagation();
    dragStateRef.current = {
      anchorId,
      mode,
      startClientX: event.clientX,
      startClientY: event.clientY,
      initItems: allItems.filter((item) => movingItemIds.includes(item.id)),
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
        const effectiveLayerDelta = clampLayerDelta(
          dragState.initItems,
          layerDelta,
        );

        // グループ全体で有効なデルタを解決（相対位置保持 + 衝突解決）
        const resolvedDelta = resolveGroupMoveDelta(
          timelineItems,
          movingIds,
          dragState.initItems,
          effectiveLayerDelta,
          desiredDelta,
        );

        setTimelineItems(
          timelineItems.map((item) => {
            const init = dragState.initItems.find((i) => i.id === item.id);
            if (!init) return item;
            const duration = init.to - init.from;
            const newFrom = Math.max(0, init.from + resolvedDelta);
            const targetLayerNum = Math.max(
              0,
              init.layer + effectiveLayerDelta,
            );
            return {
              ...item,
              from: newFrom,
              to: newFrom + duration,
              layer: targetLayerNum,
            };
          }),
        );
        return;
      }

      // resize操作（anchorId 単体のみ）
      setTimelineItems(
        timelineItems.map((item) => {
          if (item.id !== dragState.anchorId) return item;
          const init = anchorInit;

          if (dragState.mode === "resize-start") {
            const nextFrom = snapFrame(
              init.from + frameDeltaRaw,
              graduationInterval,
              zoomLevelPxPerFrame,
            );
            const desiredFrom = Math.max(0, Math.min(nextFrom, init.to - 1));

            // 左側障害物（init.toより前に終わるもの）が左方向への拡張を制限する
            const minFrom = timelineItems
              .filter(
                (o) =>
                  o.layer === item.layer &&
                  !movingIds.has(o.id) &&
                  o.to <= init.from,
              )
              .reduce((acc, o) => Math.max(acc, o.to), 0);

            return { ...item, from: Math.max(desiredFrom, minFrom) };
          }

          // resize-end
          const nextTo = snapFrame(
            init.to + frameDeltaRaw,
            graduationInterval,
            zoomLevelPxPerFrame,
          );
          const desiredTo = Math.max(init.from + 1, nextTo);

          // 右側障害物（init.to以降から始まるもの）が右方向への拡張を制限する
          const maxTo = timelineItems
            .filter(
              (o) =>
                o.layer === item.layer &&
                !movingIds.has(o.id) &&
                o.from >= init.to,
            )
            .reduce((acc, o) => Math.min(acc, o.from), Infinity);

          return { ...item, to: Math.min(desiredTo, maxTo) };
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
    setTimelineItems,
    timelineItems,
    zoomLevelPxPerFrame,
  ]);

  return timelineItems.map((item) => {
    const left = item.from * zoomLevelPxPerFrame;
    const width = (item.to - item.from) * zoomLevelPxPerFrame;
    const top = item.layer * layerHeight;
    const isSelected = selectedItemIds.includes(item.id);
    const isActiveDrag = isDragging && isSelected;

    return (
      <div
        key={item.id}
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
            if (event.ctrlKey) {
              // Ctrl+クリック: 単体の選択/非選択トグル（伝播停止で親の選択クリアを防ぐ）
              event.stopPropagation();
              if (isSelected) {
                const next = selectedItemIds.filter((id) => id !== item.id);
                setSelectedItems(next, next[0] ?? null);
              } else {
                addSelectedItemId(item.id);
              }
            } else if (event.shiftKey) {
              event.stopPropagation();
              addSelectedItemId(item.id);
            } else if (isSelected) {
              // 複数選択中のアイテムをドラッグ → 選択維持のまま全体を移動
              setIsDragging(true);
              beginItemDrag(
                event,
                item.id,
                "move",
                selectedItemIds,
                timelineItems,
              );
            } else {
              setSelectedItemId(item.id);
              setIsDragging(true);
              beginItemDrag(event, item.id, "move", [item.id], timelineItems);
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
                item.id,
                "resize-start",
                [item.id],
                timelineItems,
              );
            }}
          />
          <div
            className="absolute right-0 top-0 bottom-0 w-2 cursor-ew-resize"
            onMouseDown={(event) => {
              setIsDragging(true);
              beginItemDrag(
                event,
                item.id,
                "resize-end",
                [item.id],
                timelineItems,
              );
            }}
          />
          <span className="text-primary-content p-2 text-xs">
            {item.object.displayName}
          </span>
        </div>
      </div>
    );
  });
};

export default LayerItems;
