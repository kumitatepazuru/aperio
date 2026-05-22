import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import frameToDate from "@shared/utils/frameToDate";
import useStore, { getCurrentFrameCount } from "@shared/store";
import { useShallow } from "zustand/shallow";

type TimelineBaseUIProps = {
  zoom: number;
  zoomLevels: number[];
  horizontalScroll: number;
  onZoomChange: (nextZoom: number) => void;
  maxVisibleLayers: number;
  layerHeight: number;
  contentWidth: number;
  graduationInterval: number;
  visibleGraduationIndices: number[];
  visibleVerticalLineIndices: number[];
  children: ReactNode;
};

const TimelineBaseUI = ({
  zoom,
  zoomLevels,
  horizontalScroll,
  onZoomChange,
  maxVisibleLayers,
  layerHeight,
  contentWidth,
  graduationInterval,
  visibleGraduationIndices,
  visibleVerticalLineIndices,
  children,
}: TimelineBaseUIProps) => {
  const { frameState, viewerState, pause, setSelectedItemId } = useStore(
    useShallow((state) => ({
      frameState: state.frameState,
      viewerState: state.viewerState,
      pause: state.pause,
      setSelectedItemId: state.setSelectedItemId,
    })),
  );
  const [currentFrameCount, setCurrentFrameCount] = useState(0);

  const graduationRef = useRef<HTMLDivElement>(null);
  const [isDragging, setIsDragging] = useState(false);

  const moveLine = useCallback(
    (clientX: number) => {
      if (!graduationRef.current) return;

      const rect = graduationRef.current.getBoundingClientRect();
      const clickedX = clientX - rect.left;
      const frame = Math.max(0, Math.round(clickedX / zoomLevels[zoom]));
      pause(frame);
    },
    [pause, zoom, zoomLevels],
  );

  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (event: globalThis.MouseEvent) =>
      moveLine(event.clientX);

    const handleMouseUp = () => {
      setIsDragging(false);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);

    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDragging, moveLine, pause, zoom, zoomLevels]);

  const handleGraduationClick = (event: React.MouseEvent<HTMLDivElement>) => {
    if (!event.shiftKey) {
      setSelectedItemId(null);
    }
    setIsDragging(true);
    moveLine(event.clientX);
  };

  useEffect(() => {
    // 再生中はcurrentFrameCountを更新し続ける
    // 読み込み後すぐも呼ばれるため、これで初期のcurrentFrameCountもセットされる
    const update = async () => {
      setCurrentFrameCount(await getCurrentFrameCount());
      if (viewerState.state === "playing") {
        requestAnimationFrame(update);
      }
    };
    update();
  }, [viewerState.state]);

  return (
    <div
      className="flex"
      style={{ transform: `translateX(-${horizontalScroll}px)` }}
    >
      <div
        className="w-20 shrink-0 bg-base-100 z-20 border-r"
        style={{ transform: `translateX(${horizontalScroll}px)` }}
      >
        <div className="h-7 border-b sticky top-0 bg-base-100">
          <input
            type="range"
            min="0"
            max={zoomLevels.length - 1}
            value={zoom}
            className="range range-xs"
            step="1"
            onChange={(e) => onZoomChange(parseInt(e.target.value))}
          />
        </div>
        <div className="flex flex-col-reverse">
          {Array.from({ length: maxVisibleLayers }).map((_, index) => (
            <div
              key={index}
              className="border-b"
              style={{ height: layerHeight }}
            >
              <span className="left-1 text-xs">
                Layer {maxVisibleLayers - index - 1}
              </span>
            </div>
          ))}
        </div>
      </div>

      <div
        className="min-w-0 shrink-0 relative"
        style={{ width: contentWidth }}
        onMouseDown={handleGraduationClick}
      >
        {/* beginFrameの線 */}
        <div
          className="absolute top-0 bottom-0 border-l border-info z-20"
          style={{
            left: viewerState.beginFrame * zoomLevels[zoom],
          }}
        />

        {/* 再生中のフレームを示す線 */}
        <div
          className="absolute top-0 bottom-0 border-l border-secondary z-20"
          style={{
            left: currentFrameCount * zoomLevels[zoom],
          }}
        />

        {/* 目盛り線 */}
        <div
          className="w-full h-7 border-b sticky top-0 bg-base-100 z-10"
          ref={graduationRef}
        >
          {visibleGraduationIndices.map((index) => {
            const margin = graduationInterval * index * zoomLevels[zoom];

            return (
              <div
                key={index}
                className="absolute border-l bottom-0"
                style={{
                  left: margin,
                  height: index % 5 === 0 ? "0.75rem" : "0.5rem",
                }}
              >
                {index % 5 === 0 && (
                  <span className="absolute bottom-2 text-xs font-mono">
                    {frameToDate(graduationInterval * index, frameState.fps)}
                  </span>
                )}
              </div>
            );
          })}
        </div>

        {/* レイヤーと垂直線 */}
        <div className="relative">
          {Array.from({ length: maxVisibleLayers }).map((_, index) => (
            <div
              key={index}
              className="border-b bg-white"
              style={{ height: layerHeight }}
            />
          ))}

          {visibleVerticalLineIndices.map((index) => {
            const margin = graduationInterval * index * zoomLevels[zoom];
            return (
              <div
                key={index}
                className="absolute border-l border-base-content/20 top-0 bottom-0"
                style={{
                  left: margin,
                  height: maxVisibleLayers * layerHeight,
                }}
              />
            );
          })}

          {children}
        </div>
      </div>
    </div>
  );
};

export default TimelineBaseUI;
