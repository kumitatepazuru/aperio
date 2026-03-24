import { useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { ColorValue } from "../../types";
import { checkerStyle, toColor } from "./helpers";

export default function ColorSwatch({
  color,
  containAlpha,
  onClick,
}: {
  color: ColorValue;
  containAlpha: boolean;
  onClick: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [center, setCenter] = useState<{ x: number; y: number } | null>(null);

  const bg = `rgba(${color[0] * 255} ${color[1] * 255} ${color[2] * 255} / ${color[3]})`;

  return (
    <div
      ref={ref}
      className="relative w-5 h-5 shrink-0"
      onMouseEnter={() => {
        if (!ref.current) return;
        const r = ref.current.getBoundingClientRect();
        setCenter({ x: r.left + r.width / 2, y: r.top + r.height / 2 });
      }}
      onMouseLeave={() => setCenter(null)}
    >
      <button
        className="absolute inset-0 rounded border border-base-content/20"
        title={toColor(color).toString({ format: "hex" })}
        onClick={onClick}
      >
        {containAlpha ? (
          <div className="absolute inset-0" style={checkerStyle} />
        ) : null}
        <div className="absolute inset-0" style={{ background: bg }} />
      </button>
      {center &&
        createPortal(
          <div
            className="fixed pointer-events-none rounded border border-base-content/20 overflow-hidden"
            style={{
              width: 40,
              height: 40,
              left: center.x - 20,
              top: center.y - 20,
              zIndex: 9999,
            }}
          >
            {containAlpha ? (
              <div className="absolute inset-0" style={checkerStyle} />
            ) : null}
            <div className="absolute inset-0" style={{ background: bg }} />
          </div>,
          document.body,
        )}
    </div>
  );
}
