type Props = {
  value: boolean;
  onChange: (value: boolean) => void;
};

const Bool = ({ value, onChange }: Props) => (
  <div>
    <input
      type="checkbox"
      checked={value}
      onChange={(e) => onChange(e.target.checked)}
    />
  </div>
);

export default Bool;
