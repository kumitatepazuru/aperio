import type { RequestStructureParameter } from "native";
import type { FC } from "react";
import type { ConfigableValue } from "./utils";
import ConfigableRow from "./ConfigableRow";

const Configable: FC<{
  structures: RequestStructureParameter[];
  value: Record<string, ConfigableValue>;
  onChange?: (values: Record<string, ConfigableValue>) => void;
}> = ({ structures, value, onChange }) => {
  return (
    <div className="grid grid-cols-[5em_1fr_auto] gap-2 items-center text-sm">
      {structures.map((param) => (
        <ConfigableRow
          key={param.id}
          param={param}
          value={value[param.id]}
          onChange={(v) => onChange?.({ ...value, [param.id]: v })}
        />
      ))}
    </div>
  );
};

export default Configable;
