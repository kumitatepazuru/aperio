import type { ColorValue } from "../types";
import NumberInput from "./NumberInput";

type Props = {
  value: ColorValue;
  useAlpha: boolean;
  onChange: (value: ColorValue) => void;
};

const toHex = (v: number) =>
  Math.round(v * 255)
    .toString(16)
    .padStart(2, "0");

const Color = ({ value, useAlpha, onChange }: Props) => {
  const hexColor = `#${toHex(value.r)}${toHex(value.g)}${toHex(value.b)}`;

  return (
    <div>
      <input
        type="color"
        value={hexColor}
        onChange={(e) => {
          const hex = e.target.value;
          const r = parseInt(hex.slice(1, 3), 16) / 255;
          const g = parseInt(hex.slice(3, 5), 16) / 255;
          const b = parseInt(hex.slice(5, 7), 16) / 255;
          onChange({ ...value, r, g, b });
        }}
      />
      {useAlpha && (
        <NumberInput
          value={value.a}
          min={0}
          max={1}
          onChange={(a) =>
            onChange({ ...value, a: Math.max(0, Math.min(1, a)) })
          }
        />
      )}
    </div>
  );
};

export default Color;
