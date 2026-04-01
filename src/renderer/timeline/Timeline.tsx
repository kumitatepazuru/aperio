import { v4 as uuidv4 } from "uuid";
import { useShallow } from "zustand/shallow";
import useStore, {
  getCurrentFrameCount,
  type TimelineLayerStructure,
} from "@shared/store";
import {
  useCallback,
  type RefCallback,
  useMemo,
  useRef,
  useState,
  type WheelEvent,
} from "react";
import useContentSize from "@/hooks/useContentSize";
import LayerItems from "./LayerItems";
import TimelineBaseUI from "./TimelineBaseUI";
import { useMergeRefs } from "@floating-ui/react";

const ZoomLevels = [20, 10, 5, 2, 1, 0.5, 0.2, 0.1, 0.05, 0.02]; // フレームあたりのピクセル数

const GraduationPerFrames = [1, 2, 3, 10, 25, 50, 75, 150, 400, 800]; // ズームレベルごとの目盛りのフレーム数間隔

console.assert(
  ZoomLevels.length === GraduationPerFrames.length,
  "ZoomLevelsとgraduationPerFramesは同じ長さでなければなりません",
);

const LayerHeight = 32; // TODO: 設定で可変できるようにする
const LayerColumnWidth = 80;
const TopGraduationOverscanCount = 2;
const VerticalLineOverscanCount = 1;

const Timeline = () => {
  const [zoom, setZoom] = useState(1); // 1フレームあたりのピクセル数
  const [horizontalScroll, setHorizontalScroll] = useState(0);
  const { size, ref: contentRef } = useContentSize();
  const topScrollRef = useRef<HTMLDivElement | null>(null);
  const { timelineLayers, setTimelineLayers, setSelectedItemId } = useStore(
    useShallow((state) => ({
      timelineLayers: state.timelineLayers,
      setTimelineLayers: state.setTimelineLayers,
      setSelectedItemId: state.setSelectedItemId,
    })),
  );
  const maxVisibleFrames = useMemo(() => {
    return (
      Math.max(...timelineLayers.map((layer) => layer.to), 0) +
      Math.floor(size.width / ZoomLevels[zoom])
    ); // 表示領域に収まるフレーム数を余裕を持って計算
  }, [size.width, timelineLayers, zoom]);
  const graduationInterval = useMemo(() => GraduationPerFrames[zoom], [zoom]);
  const maxVisibleLayers = useMemo(() => {
    // 表示するレイヤー数は要素いっぱいまたは最大レイヤー番号+5のどちらか大きい方
    return Math.max(
      ...timelineLayers.map((layer) => layer.layer + 5),
      Math.ceil(size.height / LayerHeight),
    );
  }, [size.height, timelineLayers]);

  const visibleGraduationIndices = useMemo(() => {
    const visibleTimelineWidth = Math.max(0, size.width - LayerColumnWidth);
    const visibleStartPx = horizontalScroll;
    const visibleEndPx = visibleStartPx + visibleTimelineWidth;
    const graduationStepPx = graduationInterval * ZoomLevels[zoom];
    const graduationCount = Math.ceil(maxVisibleFrames / graduationInterval);

    if (graduationStepPx <= 0 || graduationCount <= 0) {
      return [];
    }

    const startIndexForTop = Math.max(
      0,
      Math.floor(visibleStartPx / graduationStepPx) -
        TopGraduationOverscanCount,
    );
    const endExclusiveIndexForTop = Math.min(
      graduationCount,
      Math.ceil(visibleEndPx / graduationStepPx) +
        TopGraduationOverscanCount +
        1,
    );

    return Array.from(
      {
        length: Math.max(0, endExclusiveIndexForTop - startIndexForTop),
      },
      (_, index) => startIndexForTop + index,
    );
  }, [
    graduationInterval,
    horizontalScroll,
    maxVisibleFrames,
    size.width,
    zoom,
  ]);

  const visibleVerticalLineIndices = useMemo(() => {
    if (ZoomLevels[zoom] < 10) {
      return [];
    }

    if (visibleGraduationIndices.length === 0) {
      return [];
    }

    const firstIndex = Math.max(
      0,
      visibleGraduationIndices[0] - VerticalLineOverscanCount,
    );
    const lastIndex =
      visibleGraduationIndices[visibleGraduationIndices.length - 1] +
      VerticalLineOverscanCount;

    return Array.from(
      { length: Math.max(0, lastIndex - firstIndex + 1) },
      (_, index) => firstIndex + index,
    );
  }, [visibleGraduationIndices, zoom]);

  const contentWidth = maxVisibleFrames * ZoomLevels[zoom];
  const totalScrollableWidth = LayerColumnWidth + contentWidth;

  const handleTopScroll = () => {
    if (!topScrollRef.current) return;
    setHorizontalScroll(topScrollRef.current.scrollLeft);
  };

  const handleBodyWheel = (event: WheelEvent<HTMLDivElement>) => {
    if (!topScrollRef.current) return;

    if (event.ctrlKey) {
      const zoomDelta = event.deltaY > 0 ? 1 : -1;
      setZoom((currentZoom) => {
        const nextZoom = currentZoom + zoomDelta;
        return Math.max(0, Math.min(ZoomLevels.length - 1, nextZoom));
      });
      return;
    }

    const horizontalDelta =
      event.deltaX !== 0 ? event.deltaX : event.shiftKey ? event.deltaY : 0;

    if (horizontalDelta === 0) return;

    topScrollRef.current.scrollLeft += horizontalDelta;
    setHorizontalScroll(topScrollRef.current.scrollLeft);
  };

  // コンテキストメニューの表示
  const contextMenuRef = useCallback<RefCallback<HTMLDivElement>>(
    (elem) => {
      if (!elem) return;

      const handleContextMenu = (event: MouseEvent) => {
        event.preventDefault();
        window.main.openContextMenu("timeline");
      };

      elem.addEventListener("contextmenu", handleContextMenu);

      const removeAddObjectListener = window.main.onAddObject(
        async (objName) => {
          console.log("Add object from context menu:", objName);

          const structure = await window.main.requestNewObjectGenerator(
            objName,
            {},
          );
          const defaultParams: Record<string, unknown> = {};
          structure.structure.forEach((param) => {
            defaultParams[param.id] = param.defaultValue;
          });

          const currentFrame = await getCurrentFrameCount();

          const endFrame = currentFrame + structure.durationFrames;
          let layer = 0;
          // 追加するオブジェクトのレイヤーは、現在のオブジェクトと重ならない最も低いレイヤーにする
          while (
            timelineLayers.some(
              (l) =>
                l.layer === layer &&
                ((currentFrame >= l.from && currentFrame < l.to) || // 現在のフレームが既存オブジェクトの範囲内
                  (endFrame > l.from && endFrame <= l.to) || // 終了フレームが既存オブジェクトの範囲内
                  (currentFrame <= l.from && endFrame >= l.to)), // 既存オブジェクトが追加するオブジェクトの範囲内
            )
          ) {
            layer++;
          }

          const newLayer: TimelineLayerStructure = {
            id: uuidv4(),
            layer,
            from: currentFrame,
            to: endFrame,
            x: 0,
            y: 0,
            scale: 100.0,
            rotation: 0.0,
            alpha: 100.0,
            obj: {
              id: uuidv4(),
              name: objName,
              displayName: structure.displayName,
              parameters: defaultParams,
            },
            effects: [],
          };

          setTimelineLayers([...timelineLayers, newLayer]);
          setSelectedItemId(newLayer.id);
        },
      );

      return () => {
        elem.removeEventListener("contextmenu", handleContextMenu);
        removeAddObjectListener();
      };
    },
    [setSelectedItemId, setTimelineLayers, timelineLayers],
  );

  return (
    <div className="h-full p-2 overflow-hidden">
      <div
        ref={topScrollRef}
        className="h-4 overflow-x-scroll overflow-y-hidden"
        onScroll={handleTopScroll}
      >
        <div style={{ width: totalScrollableWidth, height: 1 }} />
      </div>

      <div
        ref={useMergeRefs([contentRef, contextMenuRef])}
        className="h-[calc(100%-1rem)] overflow-x-hidden overflow-y-scroll"
        onWheel={handleBodyWheel}
      >
        <TimelineBaseUI
          zoom={zoom}
          zoomLevels={ZoomLevels}
          horizontalScroll={horizontalScroll}
          onZoomChange={setZoom}
          maxVisibleLayers={maxVisibleLayers}
          layerHeight={LayerHeight}
          contentWidth={contentWidth}
          graduationInterval={graduationInterval}
          visibleGraduationIndices={visibleGraduationIndices}
          visibleVerticalLineIndices={visibleVerticalLineIndices}
        >
          <LayerItems
            zoomLevelPxPerFrame={ZoomLevels[zoom]}
            graduationInterval={graduationInterval}
            layerHeight={LayerHeight}
          />
        </TimelineBaseUI>
      </div>
    </div>
  );
};

export default Timeline;
