import type { ReactNode } from "react";
import frameToDate from "@/utils/frameToDate";
import useStore from "@/store";

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
  const fps = useStore((state) => state.fps);

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

      <div className="min-w-0 shrink-0" style={{ width: contentWidth }}>
        <div className="w-full h-7 border-b sticky top-0 bg-base-100 z-10">
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
                    {frameToDate(graduationInterval * index, fps)}
                  </span>
                )}
              </div>
            );
          })}
        </div>

        <div className="relative">
          {Array.from({ length: maxVisibleLayers }).map((_, index) => (
            <div
              key={index}
              className="border-b"
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
