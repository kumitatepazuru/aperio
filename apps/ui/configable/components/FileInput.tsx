import type { RequestStructureParameter } from "native";
import type { FileValue } from "../utils";

type FileParam = Extract<RequestStructureParameter, { type: "File" }>;

type Props = {
  param: FileParam;
  value: FileValue;
  onChange: (value: FileValue) => void;
};

const FileInput = ({ param, value, onChange }: Props) => {
  const handleClick = async () => {
    const properties: Array<
      "openFile" | "openDirectory" | "multiSelections"
    > = [];
    if (param.openType === "file") properties.push("openFile");
    if (param.openType === "directory") properties.push("openDirectory");
    if (param.multiSelections) properties.push("multiSelections");

    const result = await window.main.showOpenDialog({
      title: `${param.title}を選択`,
      filters: param.filters,
      properties,
    });

    if (result !== undefined) {
      onChange(result);
    }
  };

  let label: string;
  if (value.length === 0) {
    label = "ファイルを選択";
  } else {
    const firstName = value[0].split(/[/\\]/).pop() ?? value[0];
    label =
      value.length === 1 ? firstName : `${firstName} 他 ${value.length - 1} 件`;
  }

  return (
    <div>
      <button className="btn btn-sm w-full" onClick={handleClick}>
        {label}
      </button>
    </div>
  );
};

export default FileInput;
