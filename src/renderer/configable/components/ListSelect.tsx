type Props = {
  value: string;
  values: Record<string, string>;
  onChange: (value: string) => void;
};

const ListSelect = ({ value, values, onChange }: Props) => (
  <div>
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      {Object.entries(values).map(([key, label]) => (
        <option key={key} value={key}>
          {label}
        </option>
      ))}
    </select>
  </div>
);

export default ListSelect;
