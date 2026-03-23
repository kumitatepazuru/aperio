import NumberInput from "./NumberInput";

type Props = {
  value: number;
  suffix?: string;
  onChange: (value: number) => void;
};

const Int = ({ value, suffix, onChange }: Props) => (
  <div>
    <NumberInput value={value} isInt onChange={onChange} />
    {suffix && <span>{suffix}</span>}
  </div>
);

export default Int;
