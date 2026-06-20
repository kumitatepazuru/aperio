import { useEffect, useRef, useState, type MouseEvent } from "react";
import useStore from "@shared/store";
import { useShallow } from "zustand/shallow";

type Props = {
  zoomLevelPxPerFrame: number;
  layerHeight: number;
};

const MultipleSelect = ({ zoomLevelPxPerFrame, layerHeight }: Props) => {
  const {
    timelineItems,
    selectedItemIds,
    mainSelectedItemId,
    setSelectedItems,
  } = useStore(
    useShallow((state) => ({
      timelineItems: state.timelineItems,
      selectedItemIds: state.selectedItemIds,
      mainSelectedItemId: state.mainSelectedItemId,
      setSelectedItems: state.setSelectedItems,
    })),
  );

  const overlayRef = useRef<HTMLDivElement>(null);
  const dragStartRef = useRef<{ x: number; y: number } | null>(null);
  const initialSelectionRef = useRef<{ ids: string[]; mainId: string | null }>({
    ids: [],
    mainId: null,
  });
  const firstEnteredIdRef = useRef<string | null>(null);
  const [selRect, setSelRect] = useState<{
    left: number;
    top: number;
    width: number;
    height: number;
  } | null>(null);

  useEffect(() => {
    const handleMouseMove = (event: globalThis.MouseEvent) => {
      if (!dragStartRef.current || !overlayRef.current) return;

      const bounds = overlayRef.current.getBoundingClientRect();
      const x = event.clientX - bounds.left;
      const y = event.clientY - bounds.top;
      const { x: sx, y: sy } = dragStartRef.current;

      const left = Math.min(sx, x);
      const top = Math.min(sy, y);
      const width = Math.abs(x - sx);
      const height = Math.abs(y - sy);

      setSelRect({ left, top, width, height });

      const inRect = timelineItems
        .filter((item) => {
          const itemLeft = item.start * zoomLevelPxPerFrame;
          const itemRight = item.end * zoomLevelPxPerFrame;
          const itemTop = item.layer * layerHeight;
          const itemBottom = itemTop + layerHeight;
          return (
            itemLeft < left + width &&
            itemRight > left &&
            itemTop < top + height &&
            itemBottom > top
          );
        })
        .map((item) => item.id);

      if (inRect.length > 0 && firstEnteredIdRef.current === null) {
        firstEnteredIdRef.current = inRect[0];
      }

      const mergedIds = [
        ...new Set([...initialSelectionRef.current.ids, ...inRect]),
      ];
      const mainId =
        initialSelectionRef.current.mainId ?? firstEnteredIdRef.current;

      setSelectedItems(mergedIds, mainId);
    };

    const handleMouseUp = () => {
      dragStartRef.current = null;
      firstEnteredIdRef.current = null;
      setSelRect(null);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [timelineItems, zoomLevelPxPerFrame, layerHeight, setSelectedItems]);

  const handleMouseDown = (event: MouseEvent<HTMLDivElement>) => {
    if (!event.shiftKey) return;
    event.stopPropagation();
    if (!overlayRef.current) return;
    const bounds = overlayRef.current.getBoundingClientRect();
    dragStartRef.current = {
      x: event.clientX - bounds.left,
      y: event.clientY - bounds.top,
    };
    initialSelectionRef.current = {
      ids: selectedItemIds,
      mainId: mainSelectedItemId,
    };
    firstEnteredIdRef.current = null;
    setSelRect(null);
  };

  return (
    <div
      ref={overlayRef}
      className="absolute inset-0"
      onMouseDown={handleMouseDown}
    >
      {selRect && selRect.width > 1 && selRect.height > 1 && (
        <div
          className="absolute border border-primary bg-primary/10 pointer-events-none"
          style={{
            left: selRect.left,
            top: selRect.top,
            width: selRect.width,
            height: selRect.height,
          }}
        />
      )}
    </div>
  );
};

export default MultipleSelect;
