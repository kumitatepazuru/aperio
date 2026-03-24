import type { Vec2Value } from "../types";
import NumberInput from "./NumberInput";

type Props = {
  value: Vec2Value;
  suffix?: string;
  isInt: boolean;
  onChange: (value: Vec2Value) => void;
};

const Vec2 = ({ value, suffix, isInt, onChange }: Props) => (
  <div className="flex flex-col gap-2">
    <NumberInput
      value={value[0]}
      isInt={isInt}
      prefix="X"
      suffix={suffix}
      onChange={(x) => onChange([x, value[1]])}
    />
    <NumberInput
      value={value[1]}
      isInt={isInt}
      prefix="Y"
      suffix={suffix}
      onChange={(y) => onChange([value[0], y])}
    />
  </div>
);

export default Vec2;
