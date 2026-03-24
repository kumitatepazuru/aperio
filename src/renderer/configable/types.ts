export type Vec2Value = [number, number];
export type Vec3Value = [number, number, number];
export type Vec4Value = [number, number, number, number];
export type ColorValue = [number, number, number, number];

export type ConfigableValue =
  | number
  | boolean
  | string
  | Vec2Value
  | Vec3Value
  | Vec4Value
  | ColorValue;
