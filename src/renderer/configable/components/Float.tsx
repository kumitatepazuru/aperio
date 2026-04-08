import NumberInput from "./NumberInput";

type Props = {
  value: number;
  suffix?: string;
  onChange: (value: number) => void;
};

const Float = ({ value, suffix, onChange }: Props) => (
  <div>
    <NumberInput value={value} onChange={onChange} suffix={suffix} />
  </div>
);

export default Float;
