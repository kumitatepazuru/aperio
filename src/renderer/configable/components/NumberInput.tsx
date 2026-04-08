import { useEffect, useRef, useState } from "react";

type Props = {
  value: number;
  isInt?: boolean;
  min?: number;
  max?: number;
  prefix?: string;
  suffix?: string;
  onChange: (value: number) => void;
};

const NumberInput = ({
  value,
  isInt = false,
  min,
  max,
  prefix,
  suffix,
  onChange,
}: Props) => {
  const inputRef = useRef<HTMLInputElement>(null);
  const dragState = useRef<{
    startX: number;
    startValue: number;
    moved: boolean;
  } | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [inputStr, setInputStr] = useState(String(value));
  const valueAtFocus = useRef<number>(value);

  // Sync external value changes to inputStr when not focused
  useEffect(() => {
    if (document.activeElement !== inputRef.current) {
      setInputStr(String(value));
    }
  }, [value]);

  // Document-level drag handlers
  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!dragState.current) return;
      const delta = e.clientX - dragState.current.startX;
      if (Math.abs(delta) > 2) dragState.current.moved = true;
      if (!dragState.current.moved) return;
      setIsDragging(true);
      const scale = e.shiftKey ? 0.1 : 1;
      const raw = dragState.current.startValue + delta * scale;
      let clamped = isInt ? Math.round(raw) : parseFloat(raw.toFixed(2));
      if (min !== undefined) clamped = Math.max(min, clamped);
      if (max !== undefined) clamped = Math.min(max, clamped);
      setInputStr(String(clamped));
      onChange(clamped);
    };

    const onMouseUp = () => {
      if (dragState.current && !dragState.current.moved) {
        inputRef.current?.focus();
        inputRef.current?.select();
      }
      dragState.current = null;
      setIsDragging(false);
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
    return () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };
  }, [isInt, min, max, onChange]);

  const onLabelMouseDown = (e: React.MouseEvent<HTMLLabelElement>) => {
    e.preventDefault();
    dragState.current = { startX: e.clientX, startValue: value, moved: false };
  };

  return (
    <label
      className="input input-sm w-full"
      style={{ cursor: isDragging ? "grabbing" : "ew-resize" }}
      onMouseDown={onLabelMouseDown}
    >
      {prefix && <span>{prefix}</span>}
      <div className="relative grow">
        {/* Hidden mirror span to measure value text width */}
        {suffix && (
          <div className="absolute">
            <span aria-hidden className="invisible whitespace-pre font-inherit">
              {String(value)}
            </span>
            <span className="ml-0.5 absolute">{suffix}</span>
          </div>
        )}
        <input
          ref={inputRef}
          type="number"
          value={inputStr}
          step={isInt ? 1 : 0.01}
          min={min}
          max={max}
          style={{ cursor: isDragging ? "grabbing" : "ew-resize" }}
          onFocus={() => {
            valueAtFocus.current = value;
          }}
          onChange={(e) => {
            setInputStr(e.target.value);
            const v = isInt
              ? parseInt(e.target.value, 10)
              : parseFloat(e.target.value);
            if (!isNaN(v)) onChange(v);
          }}
          onBlur={() => {
            const v = isInt ? parseInt(inputStr, 10) : parseFloat(inputStr);
            if (isNaN(v)) {
              onChange(valueAtFocus.current);
              setInputStr(String(valueAtFocus.current));
            }
          }}
        />
      </div>
    </label>
  );
};

export default NumberInput;
