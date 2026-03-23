type Props = {
  value: number;
  isInt?: boolean;
  min?: number;
  max?: number;
  prefix?: string;
  suffix?: string;
  onChange: (value: number) => void;
};

const NumberInput = ({
  value,
  isInt = false,
  min,
  max,
  prefix,
  suffix,
  onChange,
}: Props) => (
  <label className="input input-sm">
    {prefix && <span>{prefix}</span>}
    <input
      type="number"
      value={value}
      step={isInt ? 1 : 0.01}
      min={min}
      max={max}
      onChange={(e) => {
        const v = isInt
          ? parseInt(e.target.value, 10)
          : parseFloat(e.target.value);
        if (!isNaN(v)) onChange(v);
      }}
    />
    {suffix && <span>{suffix}</span>}
  </label>
);

export default NumberInput;
