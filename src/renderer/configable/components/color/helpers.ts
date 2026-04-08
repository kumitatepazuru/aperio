import type { CSSProperties } from "react";
import CjsColor from "colorjs.io";
import type { ColorValue } from "../../utils";

export const clamp01 = (x: number) => Math.max(0, Math.min(1, x));
export const nan0 = (x: number | null) => (!x || isNaN(x) ? 0 : x);

export const toColor = ([r, g, b, a]: ColorValue) =>
  new CjsColor("srgb", [r, g, b], a);

export const fromColor = (c: CjsColor, alpha: number): ColorValue => {
  const srgb = c.to("srgb").toGamut({ space: "srgb", method: "clip" });
  return [
    nan0(srgb.coords[0]),
    nan0(srgb.coords[1]),
    nan0(srgb.coords[2]),
    alpha,
  ];
};

/** Generate a CSS linear-gradient string by varying one coord of a color space. */
export function makeGradientCss(
  spaceId: string,
  coordIdx: 0 | 1 | 2,
  baseCoords: [number, number, number],
  range: [number, number],
): string {
  const [mn, mx] = range;
  const n = mx - mn >= 300 ? 12 : 1;
  const stops: string[] = [];
  for (let i = 0; i <= n; i++) {
    const t = i / n;
    const v = mn + t * (mx - mn);
    const c = [...baseCoords] as [number, number, number];
    c[coordIdx] = v;
    try {
      const srgb = new CjsColor(spaceId, c)
        .to("srgb")
        .toGamut({ space: "srgb", method: "clip" });
      stops.push(
        `rgb(${Math.round(nan0(srgb.coords[0]) * 255)},${Math.round(nan0(srgb.coords[1]) * 255)},${Math.round(nan0(srgb.coords[2]) * 255)})`,
      );
    } catch {
      stops.push("transparent");
    }
  }
  return `linear-gradient(to right, ${stops.join(", ")})`;
}

export const checkerStyle: CSSProperties = {
  backgroundImage:
    "linear-gradient(45deg,#aaa 25%,transparent 25%)," +
    "linear-gradient(-45deg,#aaa 25%,transparent 25%)," +
    "linear-gradient(45deg,transparent 75%,#aaa 75%)," +
    "linear-gradient(-45deg,transparent 75%,#aaa 75%)",
  backgroundSize: "8px 8px",
  backgroundPosition: "0 0,0 4px,4px -4px,-4px 0px",
};
