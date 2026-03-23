import type { Vec2Value } from "../types";
import NumberInput from "./NumberInput";

type Props = {
  value: Vec2Value;
  suffix?: string;
  isInt: boolean;
  onChange: (value: Vec2Value) => void;
};

const Vec2 = ({ value, suffix, isInt, onChange }: Props) => (
  <div className="flex gap-2">
    <NumberInput
      value={value.x}
      isInt={isInt}
      prefix="X"
      suffix={suffix}
      onChange={(x) => onChange({ ...value, x })}
    />
    <NumberInput
      value={value.y}
      isInt={isInt}
      prefix="Y"
      suffix={suffix}
      onChange={(y) => onChange({ ...value, y })}
    />
  </div>
);

export default Vec2;
