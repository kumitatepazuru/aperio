import { useCallback, useRef } from "react";
import NumberInput from "../NumberInput";

type ColorNumberInputProps = {
  value: number;
  isInt?: boolean;
  min: number;
  max: number;
  prefix?: string;
  suffix?: string;
  onChange: (value: number) => void;
  gradientCss: string;
};

export default function ColorNumberInput({
  value,
  isInt,
  min,
  max,
  prefix,
  suffix,
  onChange,
  gradientCss,
}: ColorNumberInputProps) {
  const sliderRef = useRef<HTMLDivElement>(null);

  const handleAt = useCallback(
    (clientX: number) => {
      if (!sliderRef.current) return;
      const rect = sliderRef.current.getBoundingClientRect();
      const t = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
      let v = min + t * (max - min);
      if (isInt) v = Math.round(v);
      onChange(v);
    },
    [min, max, isInt, onChange],
  );

  const t = max !== min ? (value - min) / (max - min) : 0;

  return (
    <div className="flex gap-1 items-center">
      {/* Gradient slider */}
      <div
        ref={sliderRef}
        className="flex-1 h-3.5 rounded relative cursor-ew-resize"
        style={{ background: gradientCss }}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          handleAt(e.clientX);
        }}
        onPointerMove={(e) => {
          if (e.pressure > 0) handleAt(e.clientX);
        }}
      >
        {/* Position indicator */}
        <div
          className="absolute top-0 bottom-0 w-0.5 pointer-events-none"
          style={{
            left: `${Math.max(0, Math.min(100, t * 100))}%`,
            transform: "translateX(-50%)",
            background: "white",
            boxShadow: "0 0 0 1px rgba(0,0,0,0.5)",
          }}
        />
      </div>
      {/* Number input */}
      <div className="w-24 shrink-0">
        <NumberInput
          value={value}
          isInt={isInt}
          min={min}
          max={max}
          prefix={prefix}
          suffix={suffix}
          onChange={onChange}
        />
      </div>
    </div>
  );
}
