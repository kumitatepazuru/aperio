import type { Vec4Value } from "../utils";
import NumberInput from "./NumberInput";

type Props = {
  value: Vec4Value;
  suffix?: string;
  isInt: boolean;
  min?: number;
  max?: number;
  onChange: (value: Vec4Value) => void;
};

const Vec4 = ({ value, suffix, isInt, min, max, onChange }: Props) => (
  <div>
    <NumberInput
      value={value[0]}
      isInt={isInt}
      prefix="X"
      suffix={suffix}
      min={min}
      max={max}
      onChange={(x) => onChange([x, value[1], value[2], value[3]])}
    />
    <NumberInput
      value={value[1]}
      isInt={isInt}
      prefix="Y"
      suffix={suffix}
      min={min}
      max={max}
      onChange={(y) => onChange([value[0], y, value[2], value[3]])}
    />
    <NumberInput
      value={value[2]}
      isInt={isInt}
      prefix="Z"
      suffix={suffix}
      min={min}
      max={max}
      onChange={(z) => onChange([value[0], value[1], z, value[3]])}
    />
    <NumberInput
      value={value[3]}
      isInt={isInt}
      prefix="W"
      suffix={suffix}
      min={min}
      max={max}
      onChange={(w) => onChange([value[0], value[1], value[2], w])}
    />
    {suffix && <span>{suffix}</span>}
  </div>
);

export default Vec4;
