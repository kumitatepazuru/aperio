import useContentSize from "@/hooks/useContentSize";
import orgFloor from "@aperio/ui/utils/orgFloor";
import useStore, { getCurrentVideoItems } from "@aperio/ui/store";
import type { ItemResult, ItemStructure } from "native";
import { useEffect, useState } from "react";
import { FaArrowRotateRight } from "react-icons/fa6";

type VideoItem = ItemStructure & { type: "Video" };

// 枠の幾何(幅・高さ・座標・拡大率・回転・中心ずれ)を取り出す。
// オフスクリーン描画済みのアイテムは result 自体がキャンバス大に焼き込み後の値なので、
// 焼き込み前の形(offscreenSource)で枠を描く。
const boxGeometry = (result: ItemResult) => result.offscreenSource ?? result;

const Overlay = () => {
  const viewerState = useStore((state) => state.viewerState);
  const frameState = useStore((state) => state.frameState);
  const frameResults = useStore((state) => state.frameResults);
  const timelineItems = useStore((state) => state.timelineItems);
  const setTimelineItems = useStore((state) => state.setTimelineItems);
  const setSelectedItemId = useStore((state) => state.setSelectedItemId);
  const addSelectedItemId = useStore((state) => state.addSelectedItemId);
  const selectedItemIds = useStore((state) => state.selectedItemIds);
  const mainSelectedItemId = useStore((state) => state.mainSelectedItemId);
  const {
    size: { width, height },
    ref: overlayRef,
  } = useContentSize();

  const [items, setItems] = useState<ItemStructure[]>([]);

  useEffect(() => {
    (async () => {
      const currentItems = await getCurrentVideoItems();
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
    const result = frameResults[itemId];
    if (!item || item.type !== "Video" || !result) return;

    // 回転中心 = 枠の transform-origin (画面上の絶対座標) をクライアント座標に変換
    // rotate button → item div → overlay div
    const overlayEl = (e.currentTarget as HTMLElement).parentElement
      ?.parentElement;
    const overlayRect = overlayEl?.getBoundingClientRect();
    if (!overlayRect) return;

    const pctW = width / frameState.width;
    const pctH = height / frameState.height;
    const box = boxGeometry(result);
    const originFrameX = box.pos[0];
    const originFrameY = box.pos[1];
    const centerClientX =
      overlayRect.left + (frameState.width / 2 + originFrameX) * pctW;
    const centerClientY =
      overlayRect.top + (frameState.height / 2 + originFrameY) * pctH;

    const initialAngle = Math.atan2(
      e.clientY - centerClientY,
      e.clientX - centerClientX,
    );

    // 選択中のすべてのVideoアイテムの初期回転角を記録
    const initRotations = new Map(
      timelineItems
        .filter(
          (i): i is VideoItem =>
            selectedItemIds.includes(i.id) && i.type === "Video",
        )
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
          if (i.type !== "Video") return i;
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
    if (!item || item.type !== "Video" || !result) return;

    const pctW = width / frameState.width;
    const pctH = height / frameState.height;

    // 枠の CSS rotate(box.rotate[2]) と同じ回転行列
    // ローカル→グローバル: [cosR, sinR; -sinR, cosR]
    // グローバル→ローカル: [cosR, -sinR; sinR, cosR]（転置）
    const box = boxGeometry(result);
    const rotRad = (box.rotate[2] * Math.PI) / 180;
    const cosR = Math.cos(rotRad);
    const sinR = Math.sin(rotRad);
    const toGlobalX = (lx: number, ly: number) => cosR * lx + sinR * ly;
    const toGlobalY = (lx: number, ly: number) => -sinR * lx + cosR * ly;
    const toLocalX = (gx: number, gy: number) => cosR * gx - sinR * gy;
    const toLocalY = (gx: number, gy: number) => sinR * gx + cosR * gy;

    const [initScaleX, initScaleY] = box.scale;
    const initX = box.pos[0];
    const initY = box.pos[1];
    const initHalfW = (box.width * initScaleX) / 2;
    const initHalfH = (box.height * initScaleY) / 2;

    // center_x/y はスケール前のテクスチャピクセル単位なのでスケールを掛ける
    const initCenterXScaled = box.centerX * initScaleX;
    const initCenterYScaled = box.centerY * initScaleY;

    const cornerSign = {
      se: { signX: 1, signY: 1 },
      nw: { signX: -1, signY: -1 },
      ne: { signX: 1, signY: -1 },
      sw: { signX: -1, signY: 1 },
    };
    const { signX, signY } = cornerSign[direction];

    // ドラッグ頂点・アンカーのローカル座標（centerオフセット込み）
    // シェーダーはテクスチャの (dims/2 + center) を box.pos にマップするため、
    // 各コーナーのローカル座標は ±halfW/H から center 分だけずれる
    const dragLocalX = signX * initHalfW - initCenterXScaled + handleOffsetX;
    const dragLocalY = signY * initHalfH - initCenterYScaled + handleOffsetY;
    const anchLocalX = -signX * initHalfW - initCenterXScaled - handleOffsetX;
    const anchLocalY = -signY * initHalfH - initCenterYScaled - handleOffsetY;

    // アンカーのグローバル（フレーム）座標 — スケーリング中ここが固定される
    const anchorX = initX + toGlobalX(anchLocalX, anchLocalY);
    const anchorY = initY + toGlobalY(anchLocalX, anchLocalY);

    // 対角線ベクトル（centerオフセットは drag-anch で相殺され影響なし = 2*sign*half）
    const diagX = dragLocalX - anchLocalX;
    const diagY = dragLocalY - anchLocalY;
    const diagLen = Math.sqrt(diagX * diagX + diagY * diagY);
    const unitX = diagX / diagLen;
    const unitY = diagY / diagLen;

    const initialMouseX = e.clientX;
    const initialMouseY = e.clientY;

    // 他の選択Videoアイテムの初期スケールを記録
    const initOtherScales = new Map(
      timelineItems
        .filter(
          (i): i is VideoItem =>
            selectedItemIds.includes(i.id) &&
            i.id !== itemId &&
            i.type === "Video",
        )
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
      const newHalfW = initHalfW * scaleFactor;
      const newHalfH = initHalfH * scaleFactor;

      // スケール後の center オフセット（テクスチャpx × 新scale）
      const newCenterXScaled = initCenterXScaled * scaleFactor;
      const newCenterYScaled = initCenterYScaled * scaleFactor;

      // アンカーを固定したまま新しい中心座標を計算
      // newCenter = anchor - toGlobal(newAnchLocal)
      // newAnchLocal = (-signX*newHalfW - newCenterXScaled, -signY*newHalfH - newCenterYScaled)
      // → newCenter = anchor + toGlobal(signX*newHalfW + newCenterXScaled, signY*newHalfH + newCenterYScaled)
      const newItemX =
        anchorX +
        toGlobalX(
          signX * newHalfW + newCenterXScaled,
          signY * newHalfH + newCenterYScaled,
        );
      const newItemY =
        anchorY +
        toGlobalY(
          signX * newHalfW + newCenterXScaled,
          signY * newHalfH + newCenterYScaled,
        );

      setTimelineItems(
        timelineItems.map((i) => {
          if (i.id === itemId) {
            if (i.type !== "Video") return i;
            return {
              ...i,
              scale: orgFloor(i.scale * scaleFactor, 100),
              // newItemX/Y は画面上の絶対位置なので、移動量だけを item.x/y へ足す
              x: Math.round(i.x + (newItemX - initX)),
              y: Math.round(i.y + (newItemY - initY)),
            };
          }
          const initS = initOtherScales.get(i.id);
          if (initS === undefined) return i;
          if (i.type !== "Video") return i;
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

    // 選択中のすべてのVideoアイテムの初期座標を記録
    const initItems = timelineItems
      .filter(
        (i): i is VideoItem =>
          selectedItemIds.includes(i.id) && i.type === "Video",
      )
      .map((i) => ({ id: i.id, x: i.x, y: i.y }));

    const onDrag = (e: MouseEvent) => {
      const deltaX = e.clientX - initialMouseX;
      const deltaY = e.clientY - initialMouseY;

      setTimelineItems(
        timelineItems.map((i) => {
          const init = initItems.find((it) => it.id === i.id);
          if (!init) return i;
          if (i.type !== "Video") return i;
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
    <div
      ref={overlayRef}
      className="absolute inset-0"
      onMouseDown={clearSelection}
    >
      {items.map((item) => {
        if (item.type !== "Video") return null;
        const result = frameResults[item.id];
        if (!result) return;
        const selected = selectedItemIds.includes(item.id);
        const mainSelected = mainSelectedItemId === item.id;

        const percentWidth = width / frameState.width;
        const percentHeight = height / frameState.height;
        // 枠は result の絶対値で描く。
        // TODO: X/Y軸回転とZ座標による遠近はCSSの矩形では表現できないため、枠はZ軸回転だけで回す。
        // オフスクリーン描画済みのアイテムは、焼き込み前の形(offscreenSource)で枠を描く。
        const box = boxGeometry(result);
        const [effScaleX, effScaleY] = box.scale;
        const [effX, effY] = box.pos;
        // scaleとborderの幅を考慮したサイズ
        const scaledWidth = box.width * effScaleX + 4;
        const scaledHeight = box.height * effScaleY + 4;
        // center_x/y はテクスチャpx単位なのでscaleを掛けてフレーム座標へ変換
        const cxScaled = box.centerX * effScaleX;
        const cyScaled = box.centerY * effScaleY;
        // バウンディングボックス中心 = (effX - cxScaled, effY - cyScaled)
        const startX = (effX - cxScaled - scaledWidth / 2) * percentWidth;
        const itemWidth = scaledWidth * percentWidth;
        const startY = (effY - cyScaled - scaledHeight / 2) * percentHeight;
        const itemHeight = scaledHeight * percentHeight;
        // 真ん中が0,0のためオフセットで修正
        const offsetX = (frameState.width / 2) * percentWidth;
        const offsetY = (frameState.height / 2) * percentHeight;
        // 回転中心 (box.pos) がdiv内のどの位置かを transform-origin に指定
        const originX = cxScaled * percentWidth + itemWidth / 2;
        const originY = cyScaled * percentHeight + itemHeight / 2;

        return (
          <div
            key={item.id}
            className="absolute border-2"
            style={{
              left: startX + offsetX,
              width: itemWidth,
              top: startY + offsetY,
              height: itemHeight,
              transform: `rotate(${box.rotate[2]}deg)`,
              transformOrigin: `${originX}px ${originY}px`,
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
