import type { Vec2Value } from "../utils";
import NumberInput from "./NumberInput";

type Props = {
  value: Vec2Value;
  suffix?: string;
  isInt: boolean;
  min?: number;
  max?: number;
  onChange: (value: Vec2Value) => void;
};

const Vec2 = ({ value, suffix, isInt, min, max, onChange }: Props) => (
  <div className="flex flex-col gap-2">
    <NumberInput
      value={value[0]}
      isInt={isInt}
      prefix="X"
      suffix={suffix}
      min={min}
      max={max}
      onChange={(x) => onChange([x, value[1]])}
    />
    <NumberInput
      value={value[1]}
      isInt={isInt}
      prefix="Y"
      suffix={suffix}
      min={min}
      max={max}
      onChange={(y) => onChange([value[0], y])}
    />
  </div>
);

export default Vec2;
