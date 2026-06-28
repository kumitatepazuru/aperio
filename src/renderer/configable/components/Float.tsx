import NumberInput from "./NumberInput";

type Props = {
  value: number;
  suffix?: string;
  min?: number;
  max?: number;
  onChange: (value: number) => void;
};

const Float = ({ value, suffix, min, max, onChange }: Props) => (
  <div>
    <NumberInput value={value} onChange={onChange} suffix={suffix} min={min} max={max} />
  </div>
);

export default Float;
