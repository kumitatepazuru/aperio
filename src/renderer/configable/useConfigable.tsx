import type { RequestStructureParameter } from "native";
import { Fragment, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { LuUndo2 } from "react-icons/lu";
import type {
  ColorValue,
  ConfigableValue,
  Vec2Value,
  Vec3Value,
  Vec4Value,
} from "./types";
import Float from "./components/Float";
import Int from "./components/Int";
import Bool from "./components/Bool";
import Vec2 from "./components/Vec2";
import Vec3 from "./components/Vec3";
import Vec4 from "./components/Vec4";
import StringInput from "./components/StringInput";
import Color from "./components/Color";
import ListSelect from "./components/ListSelect";

function getDefaultValue(param: RequestStructureParameter): ConfigableValue {
  switch (param.type) {
    case "Float":
    case "Int":
      return param.defaultValue;
    case "Bool":
      return param.defaultValue;
    case "String":
      return param.defaultValue;
    case "List":
      return param.defaultValue;
    case "Vec2Int":
    case "Vec2Float":
      return [param.defaultValue[0], param.defaultValue[1]];
    case "Vec3Int":
    case "Vec3Float":
      return [param.defaultValue[0], param.defaultValue[1], param.defaultValue[2]];
    case "Vec4Int":
    case "Vec4Float":
    case "Color":
      return [param.defaultValue[0], param.defaultValue[1], param.defaultValue[2], param.defaultValue[3]];
  }
}

function initValues(
  structures: RequestStructureParameter[],
  initial: Record<string, ConfigableValue>,
): Record<string, ConfigableValue> {
  const record: Record<string, ConfigableValue> = {};
  console.log("initValues structures", structures, "initial", initial);
  for (const param of structures) {
    record[param.id] =
      param.id in initial ? initial[param.id] : getDefaultValue(param);
  }
  return record;
}

export function useConfigable(
  structures: RequestStructureParameter[],
  initialValues: Record<string, ConfigableValue>,
  resetKey?: string,
  onChange?: (values: Record<string, ConfigableValue>) => void,
) {
  const [values, setValues] = useState<Record<string, ConfigableValue>>(() =>
    initValues(structures, initialValues),
  );

  const initialValuesRef = useRef(initialValues);
  initialValuesRef.current = initialValues;

  const handleChange = useCallback((id: string, value: ConfigableValue) => {
    setValues((prev) => {
      const next = { ...prev, [id]: value };
      onChange?.(next);
      return next;
    });
  }, [onChange]);

  const handleReset = (param: RequestStructureParameter) => {
    handleChange(param.id, getDefaultValue(param));
  };

  useEffect(() => {
    // structuresまたはアイテムが変わったときに、valuesを初期化する
    const initial = initialValuesRef.current;
    setValues(initValues(structures, initial));

    // parametersに不足しているキーをデフォルト値で補完する
    for (const param of structures) {
      if (!(param.id in initial)) {
        handleChange(param.id, getDefaultValue(param));
      }
    }
  }, [structures, resetKey, handleChange]);

  const wrap = (
    id: string,
    param: RequestStructureParameter,
    node: ReactNode,
  ) => (
    <Fragment key={id}>
      <label>{param.title}</label>
      {node}
      <button
        onClick={() => handleReset(param)}
        className="btn btn-square btn-sm h-full"
      >
        <LuUndo2 />
      </button>
    </Fragment>
  );

  const element = (
    <div className="grid grid-cols-[5em_1fr_auto] gap-2 items-center">
      {structures.map((param) => {
        const id = param.id;
        const value = values[id] ?? getDefaultValue(param);

        switch (param.type) {
          case "Float":
            return wrap(
              id,
              param,
              <Float
                value={value as number}
                suffix={param.suffix}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "Int":
            return wrap(
              id,
              param,
              <Int
                value={value as number}
                suffix={param.suffix}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "Bool":
            return wrap(
              id,
              param,
              <Bool
                value={value as boolean}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "Vec2Int":
            return wrap(
              id,
              param,
              <Vec2
                value={value as Vec2Value}
                suffix={param.suffix}
                isInt={true}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "Vec2Float":
            return wrap(
              id,
              param,
              <Vec2
                value={value as Vec2Value}
                suffix={param.suffix}
                isInt={false}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "Vec3Int":
            return wrap(
              id,
              param,
              <Vec3
                value={value as Vec3Value}
                suffix={param.suffix}
                isInt={true}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "Vec3Float":
            return wrap(
              id,
              param,
              <Vec3
                value={value as Vec3Value}
                suffix={param.suffix}
                isInt={false}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "Vec4Int":
            return wrap(
              id,
              param,
              <Vec4
                value={value as Vec4Value}
                suffix={param.suffix}
                isInt={true}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "Vec4Float":
            return wrap(
              id,
              param,
              <Vec4
                value={value as Vec4Value}
                suffix={param.suffix}
                isInt={false}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "String":
            return wrap(
              id,
              param,
              <StringInput
                value={value as string}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "Color":
            return wrap(
              id,
              param,
              <Color
                value={value as ColorValue}
                useAlpha={param.useAlpha}
                onChange={(v) => handleChange(id, v)}
              />,
            );
          case "List":
            return wrap(
              id,
              param,
              <ListSelect
                value={value as string}
                values={param.values}
                onChange={(v) => handleChange(id, v)}
              />,
            );
        }
      })}
    </div>
  );

  return { element, values };
}
