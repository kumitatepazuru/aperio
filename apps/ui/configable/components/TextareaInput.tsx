type Props = {
  value: string;
  onChange: (value: string) => void;
};

const TextareaInput = ({ value, onChange }: Props) => (
  <div>
    <textarea
      className="textarea w-full"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  </div>
);

export default TextareaInput;
