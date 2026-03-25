import type { ColorSpace } from "@shared/store";

export const CS_SPACE_ID: Record<ColorSpace, string> = {
  HSV: "hsv",
  LCH: "lch",
  okLCH: "oklch",
  LAB: "lab",
  okLAB: "oklab",
};

export const CS_LABELS: Record<ColorSpace, [string, string, string]> = {
  HSV: ["H", "S", "V"],
  LCH: ["L", "C", "H"],
  okLCH: ["L", "C", "H"],
  LAB: ["L", "a", "b"],
  okLAB: ["L", "a", "b"],
};

export const CS_RANGE: Record<
  ColorSpace,
  [[number, number], [number, number], [number, number]]
> = {
  HSV: [
    [0, 360],
    [0, 100],
    [0, 100],
  ],
  LCH: [
    [0, 100],
    [0, 150],
    [0, 360],
  ],
  okLCH: [
    [0, 1],
    [0, 0.4],
    [0, 360],
  ],
  LAB: [
    [0, 100],
    [-125, 125],
    [-125, 125],
  ],
  okLAB: [
    [0, 1],
    [-0.4, 0.4],
    [-0.4, 0.4],
  ],
};
