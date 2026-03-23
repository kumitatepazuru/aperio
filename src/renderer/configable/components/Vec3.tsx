import type { Vec3Value } from "../types";
import NumberInput from "./NumberInput";

type Props = {
  value: Vec3Value;
  suffix?: string;
  isInt: boolean;
  onChange: (value: Vec3Value) => void;
};

const Vec3 = ({ value, suffix, isInt, onChange }: Props) => (
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
  </div>
);

export default Vec3;
