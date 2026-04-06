import useContentSize from "@/hooks/useContentSize";
import orgFloor from "@/utils/orgFloor";
import useStore, {
  getCurrentFrameStruct,
  type TimelineLayerStructure,
} from "@shared/store";
import { useEffect, useState } from "react";
import { FaArrowRotateRight } from "react-icons/fa6";

const Overlay = () => {
  const viewerState = useStore((state) => state.viewerState);
  const frameState = useStore((state) => state.frameState);
  const timelineLayers = useStore((state) => state.timelineLayers);
  const setTimelineLayers = useStore((state) => state.setTimelineLayers);
  const setSelectedItemId = useStore((state) => state.setSelectedItemId);
  const selectedItemId = useStore((state) => state.selectedItemId);
  const frameResults = frameState.frameResults;
  const {
    size: { width, height },
    ref: overlayRef,
  } = useContentSize();

  const [layers, setLayers] = useState<TimelineLayerStructure[]>([]);

  useEffect(() => {
    (async () => {
      const currentLayers = await getCurrentFrameStruct();
      setLayers(currentLayers);
    })();
  }, [viewerState, frameResults, timelineLayers]);

  const onRotateStart = (
    layerId: string,
    e: React.MouseEvent<HTMLDivElement>,
  ) => {
    e.preventDefault();
    e.stopPropagation();

    const layer = timelineLayers.find((l) => l.id === layerId);
    if (!layer) return;

    // ハンドルの親要素（レイヤーdiv）の中心 = 回転中心をクライアント座標で取得
    const layerRect = (
      e.currentTarget as HTMLElement
    ).parentElement?.getBoundingClientRect();
    if (!layerRect) return;

    const centerClientX = (layerRect.left + layerRect.right) / 2;
    const centerClientY = (layerRect.top + layerRect.bottom) / 2;

    const initialAngle = Math.atan2(
      e.clientY - centerClientY,
      e.clientX - centerClientX,
    );
    const initialRotation = layer.rotation;

    const onRotate = (e: MouseEvent) => {
      const currentAngle = Math.atan2(
        e.clientY - centerClientY,
        e.clientX - centerClientX,
      );
      const deltaAngleDeg = (currentAngle - initialAngle) * (180 / Math.PI);
      // CSS: rotate(-layer.rotation) なので、時計回りドラッグ = layer.rotation 減少
      const newRotation = initialRotation - deltaAngleDeg;

      setTimelineLayers(
        timelineLayers.map((l) =>
          l.id === layerId ? { ...l, rotation: orgFloor(newRotation, 100) } : l,
        ),
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
    layerId: string,
    direction: "nw" | "ne" | "sw" | "se",
    handleOffsetX: number,
    handleOffsetY: number,
    e: React.MouseEvent<HTMLDivElement>,
  ) => {
    e.preventDefault();
    e.stopPropagation();

    const layer = timelineLayers.find((l) => l.id === layerId);
    const result = frameResults[layerId];
    if (!layer || !result) return;

    const pctW = width / frameState.width;
    const pctH = height / frameState.height;

    // CSS rotate(-layer.rotation) の回転行列
    // ローカル→グローバル: [cosR, sinR; -sinR, cosR]
    // グローバル→ローカル: [cosR, -sinR; sinR, cosR]（転置）
    const rotRad = (layer.rotation * Math.PI) / 180;
    const cosR = Math.cos(rotRad);
    const sinR = Math.sin(rotRad);
    const toGlobalX = (lx: number, ly: number) => cosR * lx + sinR * ly;
    const toGlobalY = (lx: number, ly: number) => -sinR * lx + cosR * ly;
    const toLocalX = (gx: number, gy: number) => cosR * gx - sinR * gy;
    const toLocalY = (gx: number, gy: number) => sinR * gx + cosR * gy;

    const initScale = layer.scale;
    const initX = layer.x;
    const initY = layer.y;
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

      const newScale = initScale * (newDiagLen / diagLen);
      const newHalfW = (result.width * newScale) / 200;
      const newHalfH = (result.height * newScale) / 200;

      // 新ドラッグ頂点のローカル座標（offsetはスケール対象外として固定）
      const newDragLocalX = signX * newHalfW + handleOffsetX;
      const newDragLocalY = signY * newHalfH + handleOffsetY;

      // アンカーを固定したまま新しい中心座標を計算
      const newLayerX = anchorX + toGlobalX(newDragLocalX, newDragLocalY);
      const newLayerY = anchorY + toGlobalY(newDragLocalX, newDragLocalY);

      setTimelineLayers(
        timelineLayers.map((l) =>
          l.id === layerId
            ? {
                ...l,
                scale: orgFloor(newScale, 100),
                x: Math.round(newLayerX),
                y: Math.round(newLayerY),
              }
            : l,
        ),
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
    layerId: string,
    e: React.MouseEvent<HTMLDivElement>,
  ) => {
    e.preventDefault();
    e.stopPropagation();

    const layer = timelineLayers.find((l) => l.id === layerId);
    if (!layer) return;

    const pctW = width / frameState.width;
    const pctH = height / frameState.height;
    const initX = layer.x;
    const initY = layer.y;
    const initialMouseX = e.clientX;
    const initialMouseY = e.clientY;

    const onDrag = (e: MouseEvent) => {
      const deltaX = e.clientX - initialMouseX;
      const deltaY = e.clientY - initialMouseY;

      setTimelineLayers(
        timelineLayers.map((l) =>
          l.id === layerId
            ? {
                ...l,
                x: Math.round(initX + deltaX / pctW),
                y: Math.round(initY + deltaY / pctH),
              }
            : l,
        ),
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
    <div ref={overlayRef} className="absolute inset-0" onClick={clearSelection}>
      {layers.map((layer) => {
        const result = frameResults[layer.id];
        if (!result) return;
        const selected = layer.id === selectedItemId;

        const percentWidth = width / frameState.width;
        const percentHeight = height / frameState.height;
        // scaleとborderの幅を考慮したサイズ
        const scaledWidth = (result.width * layer.scale) / 100 + 4;
        const scaledHeight = (result.height * layer.scale) / 100 + 4;
        // レイヤーの中心がframeStateの中心に来るように配置
        const startX = (layer.x - scaledWidth / 2) * percentWidth;
        const layerWidth = scaledWidth * percentWidth;
        const startY = (layer.y - scaledHeight / 2) * percentHeight;
        const layerHeight = scaledHeight * percentHeight;
        // 真ん中が0,0のためオフセットで修正
        const offsetX = (frameState.width / 2) * percentWidth;
        const offsetY = (frameState.height / 2) * percentHeight;

        return (
          <div
            key={layer.id}
            className="absolute border-2 border-primary"
            style={{
              left: startX + offsetX,
              width: layerWidth,
              top: startY + offsetY,
              height: layerHeight,
              transform: `rotate(${-layer.rotation}deg)`,
              opacity: selected ? 1 : 0,
              cursor: selected ? "move" : "default",
            }}
            onMouseDown={(e) => selected ? onDragStart(layer.id, e) : setSelectedItemId(layer.id)}
          >
            {selected && (
              <>
                <div
                  className="btn btn-circle absolute -top-8 left-[50%] -translate-x-[50%] w-6 h-6 cursor-grab active:cursor-grabbing"
                  onMouseDown={(e) => onRotateStart(layer.id, e)}
                >
                  <FaArrowRotateRight size="0.8rem" />
                </div>
                <div
                  className="absolute top-0 left-0 w-4 aspect-square rounded-full bg-base-100 border-2 border-primary -translate-[50%] cursor-nwse-resize"
                  onMouseDown={(e) => onScaleStart(layer.id, "nw", 0, 0, e)}
                />
                <div
                  className="absolute top-0 right-0 w-4 aspect-square rounded-full bg-base-100 border-2 border-primary -translate-y-[50%] translate-x-[50%] cursor-nesw-resize"
                  onMouseDown={(e) => onScaleStart(layer.id, "ne", 0, 0, e)}
                />
                <div
                  className="absolute bottom-0 left-0 w-4 aspect-square rounded-full bg-base-100 border-2 border-primary translate-y-[50%] -translate-x-[50%] cursor-nesw-resize"
                  onMouseDown={(e) => onScaleStart(layer.id, "sw", 0, 0, e)}
                />
                <div
                  className="absolute bottom-0 right-0 w-4 aspect-square rounded-full bg-base-100 border-2 border-primary translate-[50%] cursor-nwse-resize"
                  onMouseDown={(e) => onScaleStart(layer.id, "se", 0, 0, e)}
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
