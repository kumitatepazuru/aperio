import useContentSize from "@/hooks/useContentSize";
import orgFloor from "@aperio/ui/utils/orgFloor";
import useStore, { getCurrentVideoItems } from "@aperio/ui/store";
import type { ItemResult, ItemStructure } from "native";
import { useEffect, useState } from "react";
import { FaArrowRotateRight } from "react-icons/fa6";

type VideoItem = ItemStructure & { type: "Video" };
type ItemEditEvent = "reduceItemMove" | "reduceItemScale" | "reduceItemRotate";

// ドラッグ中は mousemove ごとにプラグインへイベントを投げる。処理中の呼び出しがあれば最新の1件だけを保留し、
// 完了後に実行することで、IPC が詰まったり古い結果が新しい結果を上書きしたりしないようにする。
const createLatestRunner = () => {
  let inFlight = false;
  let pending: (() => Promise<void>) | null = null;
  const run = (task: () => Promise<void>) => {
    if (inFlight) {
      pending = task;
      return;
    }
    inFlight = true;
    task()
      .catch((err) => console.error("Failed to dispatch item event:", err))
      .finally(() => {
        inFlight = false;
        const next = pending;
        pending = null;
        if (next) run(next);
      });
  };
  return run;
};

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

  // 操作ごとにイベントを呼び、結果によってその情報に基づいてアイテムを更新する
  const startItemEdit = (event: ItemEditEvent) => {
    const oldItems = items;
    const selected = new Set(selectedItemIds);
    const run = createLatestRunner();

    return (edit: (item: VideoItem) => VideoItem) => {
      const newItems = oldItems.map((i) =>
        i.type === "Video" && selected.has(i.id) ? edit(i) : i,
      );
      run(async () => {
        // ハンドラーは handler(oldItems, newItems) として呼ばれ、書き換えた newItems を返す
        const results = await window.events[event](newItems, oldItems);
        const edited = new Map(
          results
            .filter((i): i is VideoItem => i.type === "Video" && selected.has(i.id))
            .map((i) => [i.id, i]),
        );
        setTimelineItems(
          timelineItems.map((i) => {
            const r = edited.get(i.id);
            if (!r) return i;
            return {
              ...r,
              x: Math.round(r.x),
              y: Math.round(r.y),
              scale: orgFloor(r.scale, 100),
              rotation: orgFloor(r.rotation, 100),
            };
          }),
        );
      });
    };
  };

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

    const edit = startItemEdit("reduceItemRotate");

    // atan2 は ±180度で折り返すので、前回からの差分を積み上げて1周以上の回転も連続させる
    let prevAngle = Math.atan2(
      e.clientY - centerClientY,
      e.clientX - centerClientX,
    );
    let totalAngle = 0;

    const onRotate = (e: MouseEvent) => {
      const currentAngle = Math.atan2(
        e.clientY - centerClientY,
        e.clientX - centerClientX,
      );
      let diff = currentAngle - prevAngle;
      if (diff > Math.PI) diff -= 2 * Math.PI;
      else if (diff < -Math.PI) diff += 2 * Math.PI;
      totalAngle += diff;
      prevAngle = currentAngle;
      // 画面は y 下向きなので atan2 の増加 = 時計回り。枠は CSS rotate(+rotation) で、正の回転も時計回り
      const deltaAngleDeg = totalAngle * (180 / Math.PI);
      edit((i) => ({ ...i, rotation: i.rotation + deltaAngleDeg }));
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

    // 枠の CSS rotate(box.rotate[2]) と同じ回転行列(y 下向きで正 = 時計回り。frame_builder の Z軸回転とも同じ)
    // ローカル→グローバル: [cosR, -sinR; sinR, cosR]
    // グローバル→ローカル: [cosR, sinR; -sinR, cosR]（転置）
    const box = boxGeometry(result);
    const rotRad = (box.rotate[2] * Math.PI) / 180;
    const cosR = Math.cos(rotRad);
    const sinR = Math.sin(rotRad);
    const toGlobalX = (lx: number, ly: number) => cosR * lx - sinR * ly;
    const toGlobalY = (lx: number, ly: number) => sinR * lx + cosR * ly;
    const toLocalX = (gx: number, gy: number) => cosR * gx + sinR * gy;
    const toLocalY = (gx: number, gy: number) => -sinR * gx + cosR * gy;

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

    const edit = startItemEdit("reduceItemScale");

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

      // 掴んだアイテムは基点を画面上で (newItem - init) だけ動かす。
      // 他の選択アイテムは同じ倍率を各自の基点を中心に掛ける(移動なし)。
      edit((i) =>
        i.id === itemId
          ? {
              ...i,
              scale: i.scale * scaleFactor,
              x: i.x + (newItemX - initX),
              y: i.y + (newItemY - initY),
            }
          : { ...i, scale: i.scale * scaleFactor },
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

    const edit = startItemEdit("reduceItemMove");

    const onDrag = (e: MouseEvent) => {
      // 画面上(フレーム座標)の移動量
      const deltaX = (e.clientX - initialMouseX) / pctW;
      const deltaY = (e.clientY - initialMouseY) / pctH;
      edit((i) => ({ ...i, x: i.x + deltaX, y: i.y + deltaY }));
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
