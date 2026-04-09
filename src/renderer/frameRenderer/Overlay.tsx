import useContentSize from "@/hooks/useContentSize";
import orgFloor from "@/utils/orgFloor";
import useStore, {
  getCurrentFrameStruct,
  type TimelineItemStructure,
} from "@shared/store";
import { useEffect, useState } from "react";
import { FaArrowRotateRight } from "react-icons/fa6";

const Overlay = () => {
  const viewerState = useStore((state) => state.viewerState);
  const frameState = useStore((state) => state.frameState);
  const timelineItems = useStore((state) => state.timelineItems);
  const setTimelineItems = useStore((state) => state.setTimelineItems);
  const setSelectedItemId = useStore((state) => state.setSelectedItemId);
  const addSelectedItemId = useStore((state) => state.addSelectedItemId);
  const selectedItemIds = useStore((state) => state.selectedItemIds);
  const mainSelectedItemId = useStore((state) => state.mainSelectedItemId);
  const frameResults = frameState.frameResults;
  const {
    size: { width, height },
    ref: overlayRef,
  } = useContentSize();

  const [items, setItems] = useState<TimelineItemStructure[]>([]);

  useEffect(() => {
    (async () => {
      const currentItems = await getCurrentFrameStruct();
      setItems(currentItems);
    })();
  }, [viewerState, frameResults, timelineItems]);

  const onRotateStart = (
    itemId: string,
    e: React.MouseEvent<HTMLDivElement>,
  ) => {
    e.preventDefault();
    e.stopPropagation();

    const item = timelineItems.find((i) => i.id === itemId);
    if (!item) return;

    // ハンドルの親要素（アイテムdiv）の中心 = 回転中心をクライアント座標で取得
    const itemRect = (
      e.currentTarget as HTMLElement
    ).parentElement?.getBoundingClientRect();
    if (!itemRect) return;

    const centerClientX = (itemRect.left + itemRect.right) / 2;
    const centerClientY = (itemRect.top + itemRect.bottom) / 2;

    const initialAngle = Math.atan2(
      e.clientY - centerClientY,
      e.clientX - centerClientX,
    );

    // 選択中のすべてのアイテムの初期回転角を記録
    const initRotations = new Map(
      timelineItems
        .filter((i) => selectedItemIds.includes(i.id))
        .map((i) => [i.id, i.rotation]),
    );

    const onRotate = (e: MouseEvent) => {
      const currentAngle = Math.atan2(
        e.clientY - centerClientY,
        e.clientX - centerClientX,
      );
      const deltaAngleDeg = (currentAngle - initialAngle) * (180 / Math.PI);

      setTimelineItems(
        timelineItems.map((i) => {
          const initRot = initRotations.get(i.id);
          if (initRot === undefined) return i;
          // CSS: rotate(-item.rotation) なので、時計回りドラッグ = item.rotation 減少
          return { ...i, rotation: orgFloor(initRot - deltaAngleDeg, 100) };
        }),
      );
    };

    document.addEventListener("mousemove", onRotate);
    document.addEventListener(
      "mouseup",
      () => {
        document.removeEventListener("mousemove", onRotate);
      },
      { once: true },
    );
  };

  const onScaleStart = (
    itemId: string,
    direction: "nw" | "ne" | "sw" | "se",
    handleOffsetX: number,
    handleOffsetY: number,
    e: React.MouseEvent<HTMLDivElement>,
  ) => {
    e.preventDefault();
    e.stopPropagation();

    const item = timelineItems.find((i) => i.id === itemId);
    const result = frameResults[itemId];
    if (!item || !result) return;

    const pctW = width / frameState.width;
    const pctH = height / frameState.height;

    // CSS rotate(-item.rotation) の回転行列
    // ローカル→グローバル: [cosR, sinR; -sinR, cosR]
    // グローバル→ローカル: [cosR, -sinR; sinR, cosR]（転置）
    const rotRad = (item.rotation * Math.PI) / 180;
    const cosR = Math.cos(rotRad);
    const sinR = Math.sin(rotRad);
    const toGlobalX = (lx: number, ly: number) => cosR * lx + sinR * ly;
    const toGlobalY = (lx: number, ly: number) => -sinR * lx + cosR * ly;
    const toLocalX = (gx: number, gy: number) => cosR * gx - sinR * gy;
    const toLocalY = (gx: number, gy: number) => sinR * gx + cosR * gy;

    const initScale = item.scale;
    const initX = item.x;
    const initY = item.y;
    const initHalfW = (result.width * initScale) / 200;
    const initHalfH = (result.height * initScale) / 200;

    const cornerSign = {
      se: { signX: 1, signY: 1 },
      nw: { signX: -1, signY: -1 },
      ne: { signX: 1, signY: -1 },
      sw: { signX: -1, signY: 1 },
    };
    const { signX, signY } = cornerSign[direction];

    // ドラッグ頂点・アンカーのローカル座標
    const dragLocalX = signX * initHalfW + handleOffsetX;
    const dragLocalY = signY * initHalfH + handleOffsetY;
    const anchLocalX = -dragLocalX;
    const anchLocalY = -dragLocalY;

    // アンカーのグローバル（フレーム）座標 — スケーリング中ここが固定される
    const anchorX = initX + toGlobalX(anchLocalX, anchLocalY);
    const anchorY = initY + toGlobalY(anchLocalX, anchLocalY);

    // 対角線ベクトル（ローカル座標系で射影するため、ローカルで計算）
    const diagX = dragLocalX - anchLocalX;
    const diagY = dragLocalY - anchLocalY;
    const diagLen = Math.sqrt(diagX * diagX + diagY * diagY);
    const unitX = diagX / diagLen;
    const unitY = diagY / diagLen;

    const initialMouseX = e.clientX;
    const initialMouseY = e.clientY;

    // 他の選択アイテムの初期スケールを記録
    const initOtherScales = new Map(
      timelineItems
        .filter((i) => selectedItemIds.includes(i.id) && i.id !== itemId)
        .map((i) => [i.id, i.scale]),
    );

    const onScale = (e: MouseEvent) => {
      // マウス移動量をフレーム座標系に変換してからローカル座標系へ逆回転
      const globalDx = (e.clientX - initialMouseX) / pctW;
      const globalDy = (e.clientY - initialMouseY) / pctH;
      const localDx = toLocalX(globalDx, globalDy);
      const localDy = toLocalY(globalDx, globalDy);

      // 対角線方向への射影で縦横比を維持したスケーリング
      const proj = localDx * unitX + localDy * unitY;
      const newDiagLen = diagLen + proj;
      if (newDiagLen <= 0) return; // 反転防止

      const scaleFactor = newDiagLen / diagLen;
      const newScale = initScale * scaleFactor;
      const newHalfW = (result.width * newScale) / 200;
      const newHalfH = (result.height * newScale) / 200;

      // 新ドラッグ頂点のローカル座標（offsetはスケール対象外として固定）
      const newDragLocalX = signX * newHalfW + handleOffsetX;
      const newDragLocalY = signY * newHalfH + handleOffsetY;

      // アンカーを固定したまま新しい中心座標を計算
      const newItemX = anchorX + toGlobalX(newDragLocalX, newDragLocalY);
      const newItemY = anchorY + toGlobalY(newDragLocalX, newDragLocalY);

      setTimelineItems(
        timelineItems.map((i) => {
          if (i.id === itemId) {
            return {
              ...i,
              scale: orgFloor(newScale, 100),
              x: Math.round(newItemX),
              y: Math.round(newItemY),
            };
          }
          const initS = initOtherScales.get(i.id);
          if (initS === undefined) return i;
          // 他の選択アイテム: 同じスケール倍率を適用（各自の中心から拡縮）
          return { ...i, scale: orgFloor(initS * scaleFactor, 100) };
        }),
      );
    };

    document.addEventListener("mousemove", onScale);
    document.addEventListener(
      "mouseup",
      () => {
        document.removeEventListener("mousemove", onScale);
      },
      { once: true },
    );
  };

  const onDragStart = (
    _itemId: string,
    e: React.MouseEvent<HTMLDivElement>,
  ) => {
    e.preventDefault();
    e.stopPropagation();

    const pctW = width / frameState.width;
    const pctH = height / frameState.height;
    const initialMouseX = e.clientX;
    const initialMouseY = e.clientY;

    // 選択中のすべてのアイテムの初期座標を記録
    const initItems = timelineItems
      .filter((i) => selectedItemIds.includes(i.id))
      .map((i) => ({ id: i.id, x: i.x, y: i.y }));

    const onDrag = (e: MouseEvent) => {
      const deltaX = e.clientX - initialMouseX;
      const deltaY = e.clientY - initialMouseY;

      setTimelineItems(
        timelineItems.map((i) => {
          const init = initItems.find((it) => it.id === i.id);
          if (!init) return i;
          return {
            ...i,
            x: Math.round(init.x + deltaX / pctW),
            y: Math.round(init.y + deltaY / pctH),
          };
        }),
      );
    };

    document.addEventListener("mousemove", onDrag);
    document.addEventListener(
      "mouseup",
      () => {
        document.removeEventListener("mousemove", onDrag);
      },
      { once: true },
    );
  };

  const clearSelection = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return; // オーバーレイ自体がクリックされた場合のみ

    setSelectedItemId(null);
  };

  return (
    <div ref={overlayRef} className="absolute inset-0" onMouseDown={clearSelection}>
      {items.map((item) => {
        const result = frameResults[item.id];
        if (!result) return;
        const selected = selectedItemIds.includes(item.id);
        const mainSelected = mainSelectedItemId === item.id;

        const percentWidth = width / frameState.width;
        const percentHeight = height / frameState.height;
        // scaleとborderの幅を考慮したサイズ
        const scaledWidth = (result.width * item.scale) / 100 + 4;
        const scaledHeight = (result.height * item.scale) / 100 + 4;
        // アイテムの中心がframeStateの中心に来るように配置
        const startX = (item.x - scaledWidth / 2) * percentWidth;
        const itemWidth = scaledWidth * percentWidth;
        const startY = (item.y - scaledHeight / 2) * percentHeight;
        const itemHeight = scaledHeight * percentHeight;
        // 真ん中が0,0のためオフセットで修正
        const offsetX = (frameState.width / 2) * percentWidth;
        const offsetY = (frameState.height / 2) * percentHeight;

        return (
          <div
            key={item.id}
            className="absolute border-2"
            style={{
              left: startX + offsetX,
              width: itemWidth,
              top: startY + offsetY,
              height: itemHeight,
              transform: `rotate(${-item.rotation}deg)`,
              opacity: selected ? 1 : 0,
              cursor: selected ? "move" : "default",
              borderColor: mainSelected
                ? "var(--color-secondary)"
                : "var(--color-primary)",
            }}
            onMouseDown={(e) => {
              if (selected) {
                onDragStart(item.id, e);
              } else if (e.shiftKey) {
                addSelectedItemId(item.id);
              } else {
                setSelectedItemId(item.id);
              }
            }}
          >
            {selected && (
              <>
                <div
                  className="btn btn-circle absolute -top-8 left-[50%] -translate-x-[50%] w-6 h-6 cursor-grab active:cursor-grabbing"
                  onMouseDown={(e) => onRotateStart(item.id, e)}
                >
                  <FaArrowRotateRight size="0.8rem" />
                </div>
                <div
                  className="absolute top-0 left-0 w-4 aspect-square rounded-full bg-base-100 border-2 -translate-[50%] cursor-nwse-resize"
                  style={{
                    borderColor: mainSelected
                      ? "var(--color-secondary)"
                      : "var(--color-primary)",
                  }}
                  onMouseDown={(e) => onScaleStart(item.id, "nw", 0, 0, e)}
                />
                <div
                  className="absolute top-0 right-0 w-4 aspect-square rounded-full bg-base-100 border-2 -translate-y-[50%] translate-x-[50%] cursor-nesw-resize"
                  style={{
                    borderColor: mainSelected
                      ? "var(--color-secondary)"
                      : "var(--color-primary)",
                  }}
                  onMouseDown={(e) => onScaleStart(item.id, "ne", 0, 0, e)}
                />
                <div
                  className="absolute bottom-0 left-0 w-4 aspect-square rounded-full bg-base-100 border-2 translate-y-[50%] -translate-x-[50%] cursor-nesw-resize"
                  style={{
                    borderColor: mainSelected
                      ? "var(--color-secondary)"
                      : "var(--color-primary)",
                  }}
                  onMouseDown={(e) => onScaleStart(item.id, "sw", 0, 0, e)}
                />
                <div
                  className="absolute bottom-0 right-0 w-4 aspect-square rounded-full bg-base-100 border-2 translate-[50%] cursor-nwse-resize"
                  style={{
                    borderColor: mainSelected
                      ? "var(--color-secondary)"
                      : "var(--color-primary)",
                  }}
                  onMouseDown={(e) => onScaleStart(item.id, "se", 0, 0, e)}
                />
              </>
            )}
          </div>
        );
      })}
    </div>
  );
};

export default Overlay;
