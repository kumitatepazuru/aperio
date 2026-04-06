import useContentSize from "@/hooks/useContentSize";
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
    const layerRect = (e.currentTarget as HTMLElement).parentElement?.getBoundingClientRect();
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
          l.id === layerId ? { ...l, rotation: newRotation } : l,
        ),
      );
    };

    document.addEventListener("mousemove", onRotate);
    document.addEventListener(
      "mouseup",
      () => { document.removeEventListener("mousemove", onRotate); },
      { once: true },
    );
  };

  const onScaleStart = (
    layerId: string,
    direction: "nw" | "ne" | "sw" | "se",
    e: React.MouseEvent<HTMLDivElement>,
  ) => {
    e.preventDefault();
    e.stopPropagation();

    const layer = timelineLayers.find((l) => l.id === layerId);
    const result = frameResults[layerId];
    if (!layer || !result) return;

    const initialMouseX = e.clientX;
    const initialMouseY = e.clientY;
    const pctW = width / frameState.width;
    const pctH = height / frameState.height;

    const initScale = layer.scale;
    const initX = layer.x;
    const initY = layer.y;
    const initHalfW = (result.width * initScale) / 200;
    const initHalfH = (result.height * initScale) / 200;

    // 対角線上の固定頂点（アンカー）とドラッグ方向の符号
    const cornerConfig = {
      se: { anchorX: initX - initHalfW, anchorY: initY - initHalfH, signX:  1, signY:  1 },
      nw: { anchorX: initX + initHalfW, anchorY: initY + initHalfH, signX: -1, signY: -1 },
      ne: { anchorX: initX - initHalfW, anchorY: initY + initHalfH, signX:  1, signY: -1 },
      sw: { anchorX: initX + initHalfW, anchorY: initY - initHalfH, signX: -1, signY:  1 },
    };
    const { anchorX, anchorY, signX, signY } = cornerConfig[direction];

    // アンカーからドラッグ頂点への対角線ベクトル（縦横比維持に使用）
    const diagX = signX * initHalfW * 2;
    const diagY = signY * initHalfH * 2;
    const diagLen = Math.sqrt(diagX * diagX + diagY * diagY);
    const unitX = diagX / diagLen;
    const unitY = diagY / diagLen;

    const onScale = (e: MouseEvent) => {
      // マウス移動量をフレーム座標系に変換
      const dx = (e.clientX - initialMouseX) / pctW;
      const dy = (e.clientY - initialMouseY) / pctH;

      // 対角線方向への射影で縦横比を維持したスケーリング
      const proj = dx * unitX + dy * unitY;
      const newDiagLen = diagLen + proj;
      if (newDiagLen <= 0) return; // 反転防止

      const newScale = initScale * (newDiagLen / diagLen);
      const newHalfW = (result.width * newScale) / 200;
      const newHalfH = (result.height * newScale) / 200;

      // アンカーを固定したまま新しい中心座標を計算
      const newLayerX = anchorX + signX * newHalfW;
      const newLayerY = anchorY + signY * newHalfH;

      setTimelineLayers(
        timelineLayers.map((l) =>
          l.id === layerId
            ? { ...l, scale: newScale, x: Math.round(newLayerX), y: Math.round(newLayerY) }
            : l,
        ),
      );
    };

    document.addEventListener("mousemove", onScale);
    document.addEventListener(
      "mouseup",
      () => { document.removeEventListener("mousemove", onScale); },
      { once: true },
    );
  };

  return (
    <div ref={overlayRef} className="absolute inset-0">
      {layers.map((layer) => {
        const result = frameResults[layer.id];
        if (!result || layer.id !== selectedItemId) return;

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
            }}
          >
            <div
              className="btn btn-circle absolute -top-8 left-[50%] -translate-x-[50%] w-6 h-6 cursor-grab active:cursor-grabbing"
              onMouseDown={(e) => onRotateStart(layer.id, e)}
            >
              <FaArrowRotateRight size="0.8rem" />
            </div>
            <div
              className="absolute top-0 left-0 w-4 aspect-square rounded-full bg-base-100 border-2 border-primary -translate-[50%] cursor-nwse-resize"
              onMouseDown={(e) => onScaleStart(layer.id, "nw", e)}
            />
            <div
              className="absolute top-0 right-0 w-4 aspect-square rounded-full bg-base-100 border-2 border-primary -translate-y-[50%] translate-x-[50%] cursor-nesw-resize"
              onMouseDown={(e) => onScaleStart(layer.id, "ne", e)}
            />
            <div
              className="absolute bottom-0 left-0 w-4 aspect-square rounded-full bg-base-100 border-2 border-primary translate-y-[50%] -translate-x-[50%] cursor-nesw-resize"
              onMouseDown={(e) => onScaleStart(layer.id, "sw", e)}
            />
            <div
              className="absolute bottom-0 right-0 w-4 aspect-square rounded-full bg-base-100 border-2 border-primary translate-[50%] cursor-nwse-resize"
              onMouseDown={(e) => onScaleStart(layer.id, "se", e)}
            />
          </div>
        );
      })}
    </div>
  );
};

export default Overlay;
