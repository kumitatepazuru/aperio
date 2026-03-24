import type { Vec4Value } from "../types";
import NumberInput from "./NumberInput";

type Props = {
  value: Vec4Value;
  suffix?: string;
  isInt: boolean;
  onChange: (value: Vec4Value) => void;
};

const Vec4 = ({ value, suffix, isInt, onChange }: Props) => (
  <div>
    <NumberInput
      value={value[0]}
      isInt={isInt}
      prefix="X"
      suffix={suffix}
      onChange={(x) => onChange([x, value[1], value[2], value[3]])}
    />
    <NumberInput
      value={value[1]}
      isInt={isInt}
      prefix="Y"
      suffix={suffix}
      onChange={(y) => onChange([value[0], y, value[2], value[3]])}
    />
    <NumberInput
      value={value[2]}
      isInt={isInt}
      prefix="Z"
      suffix={suffix}
      onChange={(z) => onChange([value[0], value[1], z, value[3]])}
    />
    <NumberInput
      value={value[3]}
      isInt={isInt}
      prefix="W"
      suffix={suffix}
      onChange={(w) => onChange([value[0], value[1], value[2], w])}
    />
    {suffix && <span>{suffix}</span>}
  </div>
);

export default Vec4;
