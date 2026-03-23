export type Vec2Value = { x: number; y: number };
export type Vec3Value = { x: number; y: number; z: number };
export type Vec4Value = { x: number; y: number; z: number; w: number };
export type ColorValue = { r: number; g: number; b: number; a: number };

export type ConfigableValue =
  | number
  | boolean
  | string
  | Vec2Value
  | Vec3Value
  | Vec4Value
  | ColorValue;
