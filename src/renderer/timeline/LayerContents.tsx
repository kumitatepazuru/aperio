import useStore from "@/store";
import { useShallow } from "zustand/shallow";
import { useCallback, useEffect, useRef, type MouseEvent } from "react";

const GraduationSnapThresholdPx = 8;

type DragMode = "move" | "resize-start" | "resize-end";

type DragState = {
  layerId: string;
  mode: DragMode;
  startClientX: number;
  startClientY: number;
  startFrom: number;
  startTo: number;
  startLayer: number;
};

type LayerContentsProps = {
  zoomLevelPxPerFrame: number;
  graduationInterval: number;
  layerHeight: number;
};

const snapFrame = (
  rawFrame: number,
  graduationInterval: number,
  zoomLevelPxPerFrame: number,
) => {
  const graduationFrame =
    Math.round(rawFrame / graduationInterval) * graduationInterval;
  const graduationDistancePx =
    Math.abs(rawFrame - graduationFrame) * zoomLevelPxPerFrame;

  if (graduationDistancePx <= GraduationSnapThresholdPx) {
    return graduationFrame;
  }

  return Math.round(rawFrame);
};

const LayerContents = ({
  zoomLevelPxPerFrame,
  graduationInterval,
  layerHeight,
}: LayerContentsProps) => {
  const { timelineLayers, setTimelineLayers } = useStore(
    useShallow((state) => ({
      timelineLayers: state.timelineLayers,
      setTimelineLayers: state.setTimelineLayers,
    })),
  );
  const dragStateRef = useRef<DragState | null>(null);

  const beginLayerDrag = useCallback(
    (
      event: MouseEvent<HTMLDivElement>,
      layerId: string,
      mode: DragMode,
      startLayerValues: { from: number; to: number; layer: number },
    ) => {
      event.preventDefault();
      event.stopPropagation();
      dragStateRef.current = {
        layerId,
        mode,
        startClientX: event.clientX,
        startClientY: event.clientY,
        startFrom: startLayerValues.from,
        startTo: startLayerValues.to,
        startLayer: startLayerValues.layer,
      };
    },
    [],
  );

  useEffect(() => {
    const handleMouseMove = (event: globalThis.MouseEvent) => {
      const dragState = dragStateRef.current;
      if (!dragState) return;

      const frameDeltaRaw =
        (event.clientX - dragState.startClientX) / zoomLevelPxPerFrame;
      const layerDelta = Math.round(
        (event.clientY - dragState.startClientY) / layerHeight,
      );

      const duration = dragState.startTo - dragState.startFrom;

      setTimelineLayers(
        timelineLayers.map((layer) => {
          if (layer.id !== dragState.layerId) return layer;

          if (dragState.mode === "move") {
            const nextFrom = Math.max(
              0,
              snapFrame(
                dragState.startFrom + frameDeltaRaw,
                graduationInterval,
                zoomLevelPxPerFrame,
              ),
            );
            const nextLayer = Math.max(0, dragState.startLayer + layerDelta);
            return {
              ...layer,
              from: nextFrom,
              to: nextFrom + duration,
              layer: nextLayer,
            };
          }

          if (dragState.mode === "resize-start") {
            const nextFrom = snapFrame(
              dragState.startFrom + frameDeltaRaw,
              graduationInterval,
              zoomLevelPxPerFrame,
            );
            return {
              ...layer,
              from: Math.max(0, Math.min(nextFrom, dragState.startTo - 1)),
            };
          }

          const nextTo = snapFrame(
            dragState.startTo + frameDeltaRaw,
            graduationInterval,
            zoomLevelPxPerFrame,
          );
          return {
            ...layer,
            to: Math.max(dragState.startFrom + 1, nextTo),
          };
        }),
      );
    };

    const handleMouseUp = () => {
      dragStateRef.current = null;
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
    setTimelineLayers,
    timelineLayers,
    zoomLevelPxPerFrame,
  ]);

  return timelineLayers.map((layer) => {
    const left = layer.from * zoomLevelPxPerFrame;
    const width = (layer.to - layer.from) * zoomLevelPxPerFrame;
    const top = layer.layer * layerHeight;

    return (
      <div
        key={layer.id}
        className="absolute py-0.5"
        style={{
          left,
          width,
          top,
          height: layerHeight,
        }}
      >
        <div
          className="bg-primary w-full h-full rounded border border-primary relative cursor-grab active:cursor-grabbing active:bg-primary/70"
          onMouseDown={(event) =>
            beginLayerDrag(event, layer.id, "move", {
              from: layer.from,
              to: layer.to,
              layer: layer.layer,
            })
          }
        >
          <div
            className="absolute left-0 top-0 bottom-0 w-2 cursor-ew-resize"
            onMouseDown={(event) =>
              beginLayerDrag(event, layer.id, "resize-start", {
                from: layer.from,
                to: layer.to,
                layer: layer.layer,
              })
            }
          />
          <div
            className="absolute right-0 top-0 bottom-0 w-2 cursor-ew-resize"
            onMouseDown={(event) =>
              beginLayerDrag(event, layer.id, "resize-end", {
                from: layer.from,
                to: layer.to,
                layer: layer.layer,
              })
            }
          />
        </div>
      </div>
    );
  });
};

export default LayerContents;
