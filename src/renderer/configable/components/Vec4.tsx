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
    <NumberInput
      value={value.z}
      isInt={isInt}
      prefix="Z"
      suffix={suffix}
      onChange={(z) => onChange({ ...value, z })}
    />
    <NumberInput
      value={value.w}
      isInt={isInt}
      prefix="W"
      suffix={suffix}
      onChange={(w) => onChange({ ...value, w })}
    />
    {suffix && <span>{suffix}</span>}
  </div>
);

export default Vec4;
