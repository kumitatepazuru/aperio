import { useShallow } from "zustand/shallow";
import useStore from "@shared/store";
import { useMemo, useRef, useState, type WheelEvent } from "react";
import useContentSize from "@/hooks/useContentSize";
import LayerContents from "./LayerContents";
import TimelineBaseUI from "./TimelineBaseUI";

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
  const { timelineLayers } = useStore(
    useShallow((state) => ({
      timelineLayers: state.timelineLayers,
    })),
  );
  const maxVisibleFrames = useMemo(() => {
    if (timelineLayers.length === 0) return 0;
    return (
      Math.max(...timelineLayers.map((layer) => layer.to)) +
      Math.floor(300 / ZoomLevels[zoom])
    ); // 300ピクセル分の余裕を持たす
  }, [timelineLayers, zoom]);
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
        ref={contentRef}
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
          <LayerContents
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
