import type { RequestStructureParameter } from "native";

export type Vec2Value = [number, number];
export type Vec3Value = [number, number, number];
export type Vec4Value = [number, number, number, number];
export type ColorValue = [number, number, number, number];
export type FontValue = { family: string | null; weight: number };
export type FileValue = string[];

export type ConfigableValue =
  | number
  | boolean
  | string
  | Vec2Value
  | Vec3Value
  | Vec4Value
  | ColorValue
  | FontValue
  | FileValue;

export const getDefaultValue = (
  param: RequestStructureParameter,
): ConfigableValue => {
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
      return [
        param.defaultValue[0],
        param.defaultValue[1],
        param.defaultValue[2],
      ];
    case "Vec4Int":
    case "Vec4Float":
    case "Color":
      return [
        param.defaultValue[0],
        param.defaultValue[1],
        param.defaultValue[2],
        param.defaultValue[3],
      ];
    case "Font":
      return { family: null, weight: 400 };
    case "Textarea":
      return param.defaultValue;
    case "File":
      return [];
  }
};

export const initValues = (
  structures: RequestStructureParameter[],
  initial: Record<string, ConfigableValue>,
): Record<string, ConfigableValue> => {
  const record: Record<string, ConfigableValue> = {};
  for (const param of structures) {
    record[param.id] =
      param.id in initial ? initial[param.id] : getDefaultValue(param);
  }
  return record;
};
