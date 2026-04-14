import { memo, type ReactNode } from "react";
import { LuUndo2 } from "react-icons/lu";
import {
  getDefaultValue,
  type ColorValue,
  type ConfigableValue,
  type FontValue,
  type Vec2Value,
  type Vec3Value,
  type Vec4Value,
} from "./utils";
import Float from "./components/Float";
import Int from "./components/Int";
import Bool from "./components/Bool";
import Vec2 from "./components/Vec2";
import Vec3 from "./components/Vec3";
import Vec4 from "./components/Vec4";
import StringInput from "./components/StringInput";
import Color from "./components/Color";
import ListSelect from "./components/ListSelect";
import Font from "./components/Font";
import type { RequestStructureParameter } from "native";

const ConfigableRow = memo(
  ({
    param,
    value,
    onChange,
  }: {
    param: RequestStructureParameter;
    value: ConfigableValue;
    onChange: (value: ConfigableValue) => void;
  }) => {
    let input: ReactNode;
    switch (param.type) {
      case "Float":
        input = (
          <Float
            value={value as number}
            suffix={param.suffix}
            onChange={onChange}
          />
        );
        break;
      case "Int":
        input = (
          <Int
            value={value as number}
            suffix={param.suffix}
            onChange={onChange}
          />
        );
        break;
      case "Bool":
        input = <Bool value={value as boolean} onChange={onChange} />;
        break;
      case "Vec2Int":
        input = (
          <Vec2
            value={value as Vec2Value}
            suffix={param.suffix}
            isInt={true}
            onChange={onChange}
          />
        );
        break;
      case "Vec2Float":
        input = (
          <Vec2
            value={value as Vec2Value}
            suffix={param.suffix}
            isInt={false}
            onChange={onChange}
          />
        );
        break;
      case "Vec3Int":
        input = (
          <Vec3
            value={value as Vec3Value}
            suffix={param.suffix}
            isInt={true}
            onChange={onChange}
          />
        );
        break;
      case "Vec3Float":
        input = (
          <Vec3
            value={value as Vec3Value}
            suffix={param.suffix}
            isInt={false}
            onChange={onChange}
          />
        );
        break;
      case "Vec4Int":
        input = (
          <Vec4
            value={value as Vec4Value}
            suffix={param.suffix}
            isInt={true}
            onChange={onChange}
          />
        );
        break;
      case "Vec4Float":
        input = (
          <Vec4
            value={value as Vec4Value}
            suffix={param.suffix}
            isInt={false}
            onChange={onChange}
          />
        );
        break;
      case "String":
        input = <StringInput value={value as string} onChange={onChange} />;
        break;
      case "Color":
        input = (
          <Color
            value={value as ColorValue}
            useAlpha={param.useAlpha}
            onChange={onChange}
          />
        );
        break;
      case "List":
        input = (
          <ListSelect
            value={value as string}
            values={param.values}
            onChange={onChange}
          />
        );
        break;
      case "Font":
        input = (
          <Font
            value={value as FontValue}
            onChange={onChange}
          />
        );
        break;
    }

    return (
      <>
        <label>{param.title}</label>
        {input}
        <button
          onClick={() => onChange(getDefaultValue(param))}
          className="btn btn-square btn-sm h-full min-h-8"
        >
          <LuUndo2 />
        </button>
      </>
    );
  },
);

export default ConfigableRow;
