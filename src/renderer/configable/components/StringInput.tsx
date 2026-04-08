type Props = {
  value: string;
  onChange: (value: string) => void;
};

const StringInput = ({ value, onChange }: Props) => (
  <div>
    <input
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  </div>
);

export default StringInput;
