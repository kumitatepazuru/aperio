import { useCallback, useEffect, useRef, useState } from "react";
import CjsColor from "colorjs.io";
import { CgColorPicker } from "react-icons/cg";
import type { ColorValue } from "../../types";
import {
  checkerStyle,
  clamp01,
  fromColor,
  makeGradientCss,
  nan0,
  toColor,
} from "./helpers";
import {
  CS_LABELS,
  CS_RANGE,
  CS_SPACE_ID,
  type ColorSpace,
  type DisplayMode,
} from "./types";
import ColorNumberInput from "./ColorNumberInput";
import ColorSwatch from "./ColorSwatch";
import SwatchPalette from "./SwatchPalette";

type Props = {
  value: ColorValue;
  useAlpha: boolean;
  onChange: (value: ColorValue) => void;
};

export default function ColorPickerPanel({ value, useAlpha, onChange }: Props) {
  const [displayMode, setDisplayMode] = useState<DisplayMode>("0-1");
  const [colorSpace, setColorSpace] = useState<ColorSpace>("HSV");
  const [history, setHistory] = useState<ColorValue[]>([]);
  const [hexFocused, setHexFocused] = useState(false);
  const [hexDraft, setHexDraft] = useState("");

  const svRef = useRef<HTMLDivElement>(null);
  const hueRef = useRef<HTMLDivElement>(null);
  const draggingSV = useRef(false);
  const draggingHue = useRef(false);

  // ─── HSV for SV/Hue picker ──────────────────────────────────────────────

  const hsvColor = toColor(value).to("hsv");
  const hsvCoords = hsvColor.coords; // [h:0-360, s:0-100, v:0-100]
  const h = nan0(hsvCoords[0]);
  const s = nan0(hsvCoords[1]) / 100;
  const v = nan0(hsvCoords[2]) / 100;

  // ─── Pointer drag handlers ──────────────────────────────────────────────

  const handleSVAt = useCallback(
    (clientX: number, clientY: number) => {
      if (!svRef.current) return;
      const rect = svRef.current.getBoundingClientRect();
      const sx = clamp01((clientX - rect.left) / rect.width);
      const sy = clamp01((clientY - rect.top) / rect.height);
      onChange(
        fromColor(new CjsColor("hsv", [h, sx * 100, (1 - sy) * 100]), value[3]),
      );
    },
    [h, value, onChange],
  );

  const handleHueAt = useCallback(
    (clientY: number) => {
      if (!hueRef.current) return;
      const rect = hueRef.current.getBoundingClientRect();
      const hy = clamp01((clientY - rect.top) / rect.height);
      onChange(
        fromColor(
          new CjsColor("hsv", [hy * 360, hsvCoords[1], hsvCoords[2]]),
          value[3],
        ),
      );
    },
    [hsvCoords, value, onChange],
  );

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      if (draggingSV.current) handleSVAt(e.clientX, e.clientY);
      if (draggingHue.current) handleHueAt(e.clientY);
    };
    const onUp = () => {
      draggingSV.current = false;
      draggingHue.current = false;
    };
    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
    return () => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };
  }, [handleSVAt, handleHueAt]);

  // ─── Color space params ─────────────────────────────────────────────────

  const spaceId = CS_SPACE_ID[colorSpace];
  const rawCoords = toColor(value).to(spaceId).coords;
  const csCoords: [number, number, number] = [
    nan0(rawCoords[0]),
    nan0(rawCoords[1]),
    nan0(rawCoords[2]),
  ];
  const csRanges = CS_RANGE[colorSpace];
  const csLabels = CS_LABELS[colorSpace];

  const handleCSChange = (idx: 0 | 1 | 2, raw: number) => {
    const next = [...csCoords] as [number, number, number];
    next[idx] = raw;
    onChange(fromColor(new CjsColor(spaceId, next), value[3]));
  };

  // ─── Display format helpers ─────────────────────────────────────────────

  const is255 = displayMode === "0-255";
  const rgbRange: [number, number] = is255 ? [0, 255] : [0, 1];

  const fmtRgb = (c: number) =>
    is255 ? Math.round(c * 255) : parseFloat(c.toFixed(4));
  const unFmtRgb = (dv: number) => (is255 ? dv / 255 : dv);

  const fmtCS = (val: number, idx: 0 | 1 | 2) => {
    if (!is255) return parseFloat(val.toFixed(4));
    const [mn, mx] = csRanges[idx];
    return Math.round(((val - mn) / (mx - mn)) * 255);
  };
  const unFmtCS = (dv: number, idx: 0 | 1 | 2) => {
    if (!is255) return dv;
    const [mn, mx] = csRanges[idx];
    return (dv / 255) * (mx - mn) + mn;
  };

  // ─── HEX ────────────────────────────────────────────────────────────────

  const hexBase = toColor(value).toString({ format: "hex" }).slice(1);
  const hexWithAlpha = useAlpha
    ? hexBase +
      Math.round(clamp01(value[3]) * 255)
        .toString(16)
        .padStart(2, "0")
    : hexBase;

  const applyHex = (raw: string) => {
    try {
      const parsed = new CjsColor(raw.startsWith("#") ? raw : `#${raw}`).to(
        "srgb",
      );
      const a =
        useAlpha && parsed.alpha !== undefined ? parsed.alpha : value[3];
      onChange([
        nan0(parsed.coords[0]),
        nan0(parsed.coords[1]),
        nan0(parsed.coords[2]),
        a,
      ]);
    } catch {
      /* invalid input — ignore */
    }
  };

  // ─── EyeDropper ─────────────────────────────────────────────────────────

  const hasEyeDropper = "EyeDropper" in window;
  const pickColor = async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const result = await new (window as any).EyeDropper()
      .open()
      .catch(() => null);
    if (!result) return;
    const parsed = new CjsColor(result.sRGBHex).to("srgb");
    onChange([
      nan0(parsed.coords[0]),
      nan0(parsed.coords[1]),
      nan0(parsed.coords[2]),
      value[3],
    ]);
  };

  // ─── Gradient CSS for ColorNumberInput ──────────────────────────────────

  const rgbBase: [number, number, number] = [value[0], value[1], value[2]];
  const rgbGradients = [0, 1, 2].map((i) =>
    makeGradientCss("srgb", i as 0 | 1 | 2, rgbBase, [0, 1]),
  );
  const [r255, g255, b255] = rgbBase.map((c) => Math.round(c * 255));
  const alphaGradientCss = `linear-gradient(to right, rgba(${r255},${g255},${b255},0), rgb(${r255},${g255},${b255}))`;
  const csGradients = csLabels.map((_, idx) =>
    makeGradientCss(spaceId, idx as 0 | 1 | 2, csCoords, csRanges[idx]),
  );

  const previewRgb = `rgb(${value[0] * 255} ${value[1] * 255} ${value[2] * 255})`;
  const previewRgba = `rgba(${value[0] * 255} ${value[1] * 255} ${value[2] * 255} / ${value[3]})`;

  return (
    /* ── Tab bar ───────────────────────────────────────────────────────── */
    <div className="tabs tabs-border">
      <input
        type="radio"
        name="color_panel"
        className="tab w-0"
        aria-label="詳細"
        defaultChecked
      />
      {/* ── Detail view ────────────────────────────────────────────────── */}
      <div className="tab-content pt-4">
        <div className="flex gap-3">
          {/* ── Left panel ────────────────────────────────────────────────── */}
          <div className="flex flex-col gap-2 flex-0">
            <div className="flex gap-2">
              {/* SV picker */}
              <div
                ref={svRef}
                className="w-40 h-40 relative rounded cursor-crosshair"
                style={{ background: `hsl(${h},100%,50%)` }}
                onPointerDown={(e) => {
                  draggingSV.current = true;
                  e.currentTarget.setPointerCapture(e.pointerId);
                  handleSVAt(e.clientX, e.clientY);
                }}
              >
                <div
                  className="absolute inset-0 rounded"
                  style={{
                    background: "linear-gradient(to right,white,transparent)",
                  }}
                />
                <div
                  className="absolute inset-0 rounded"
                  style={{
                    background: "linear-gradient(to top,black,transparent)",
                  }}
                />
                <div
                  className="absolute w-3 h-3 rounded-full border-2 border-white pointer-events-none -translate-[50%]"
                  style={{
                    left: `${s * 100}%`,
                    top: `${(1 - v) * 100}%`,
                    boxShadow: "0 0 0 1px rgba(0,0,0,0.6)",
                  }}
                />
              </div>

              {/* Hue bar */}
              <div
                ref={hueRef}
                className="w-5 h-40 rounded relative shrink-0 cursor-ns-resize"
                style={{
                  background:
                    "linear-gradient(to bottom,#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00)",
                }}
                onPointerDown={(e) => {
                  draggingHue.current = true;
                  e.currentTarget.setPointerCapture(e.pointerId);
                  handleHueAt(e.clientY);
                }}
              >
                <div
                  className="absolute left-0 right-0 pointer-events-none -translate-y-[50%] h-1.5"
                  style={{
                    top: `${(h / 360) * 100}%`,
                    border: "1.5px solid white",
                    boxShadow: "0 0 0 1px rgba(0,0,0,0.6)",
                  }}
                />
              </div>
            </div>

            {/* Color preview */}
            <div className="flex rounded overflow-hidden h-6">
              <div className="flex-1" style={{ background: previewRgb }} />
              <div className="flex-1 relative">
                <div className="absolute inset-0" style={checkerStyle} />
                <div
                  className="absolute inset-0"
                  style={{ background: previewRgba }}
                />
              </div>
            </div>

            {/* History */}
            <div className="flex gap-1">
              <button
                className="btn btn-xs btn-square"
                title="現在の色を履歴に追加"
                onClick={() =>
                  setHistory((prev) => [[...value] as ColorValue, ...prev])
                }
              >
                +
              </button>
              <div className="flex flex-wrap gap-1 overflow-y-auto max-h-28 grow">
                {history.map((c, i) => (
                  <ColorSwatch
                    key={i}
                    color={c}
                    onClick={() => onChange(c)}
                    containAlpha={true}
                  />
                ))}
              </div>
            </div>
          </div>

          {/* ── Right panel ───────────────────────────────────────────────── */}
          <div className="flex flex-col gap-2 flex-1">
            {/* Section 1: display mode + RGB(A) */}
            <div className="flex flex-col gap-1">
              <div className="join w-full">
                {(["0-1", "0-255"] as DisplayMode[]).map((m) => (
                  <button
                    key={m}
                    className={`join-item btn btn-xs flex-1 ${displayMode === m ? "btn-primary" : ""}`}
                    onClick={() => setDisplayMode(m)}
                  >
                    {m}
                  </button>
                ))}
              </div>
              {(["R", "G", "B"] as const).map((label, i) => (
                <ColorNumberInput
                  key={label}
                  value={fmtRgb(value[i])}
                  min={rgbRange[0]}
                  max={rgbRange[1]}
                  isInt={is255}
                  prefix={label}
                  gradientCss={rgbGradients[i]}
                  onChange={(dv) => {
                    const next: ColorValue = [...value];
                    next[i] = clamp01(unFmtRgb(dv));
                    onChange(next);
                  }}
                />
              ))}
              {useAlpha && (
                <ColorNumberInput
                  value={
                    is255
                      ? Math.round(value[3] * 255)
                      : parseFloat(value[3].toFixed(4))
                  }
                  min={rgbRange[0]}
                  max={rgbRange[1]}
                  isInt={is255}
                  prefix="A"
                  gradientCss={alphaGradientCss}
                  onChange={(dv) =>
                    onChange([
                      value[0],
                      value[1],
                      value[2],
                      clamp01(is255 ? dv / 255 : dv),
                    ])
                  }
                />
              )}
            </div>

            {/* Section 2: color space selector + params */}
            <div className="flex flex-col gap-1">
              <div className="join w-full">
                {(["HSV", "LCH", "okLCH", "LAB", "okLAB"] as ColorSpace[]).map(
                  (cs) => (
                    <button
                      key={cs}
                      className={`join-item btn btn-xs flex-1 ${colorSpace === cs ? "btn-primary" : ""}`}
                      onClick={() => setColorSpace(cs)}
                    >
                      {cs}
                    </button>
                  ),
                )}
              </div>
              {csLabels.map((label, idx) => (
                <ColorNumberInput
                  key={colorSpace + label + idx}
                  value={fmtCS(csCoords[idx], idx as 0 | 1 | 2)}
                  min={is255 ? 0 : csRanges[idx][0]}
                  max={is255 ? 255 : csRanges[idx][1]}
                  isInt={is255}
                  prefix={label}
                  gradientCss={csGradients[idx]}
                  onChange={(dv) =>
                    handleCSChange(
                      idx as 0 | 1 | 2,
                      unFmtCS(dv, idx as 0 | 1 | 2),
                    )
                  }
                />
              ))}
            </div>

            {/* Section 3: HEX + EyeDropper */}
            <div className="flex flex-col gap-1">
              <div className="flex gap-1">
                <label className="input input-sm flex-1 min-w-0 font-mono">
                  <span className="opacity-50">#</span>
                  <input
                    type="text"
                    className="w-full"
                    value={hexFocused ? hexDraft : hexWithAlpha}
                    onFocus={() => {
                      setHexFocused(true);
                      setHexDraft(hexWithAlpha);
                    }}
                    onBlur={() => {
                      applyHex(hexDraft);
                      setHexFocused(false);
                    }}
                    onChange={(e) => {
                      setHexDraft(e.target.value);
                      applyHex(e.target.value);
                    }}
                  />
                </label>
                {hasEyeDropper && (
                  <button
                    className="btn btn-sm btn-square"
                    title="画面から色を取得"
                    onClick={pickColor}
                  >
                    <CgColorPicker size="1.5em" />
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>

      <input
        type="radio"
        name="color_panel"
        aria-label="色見本から"
        className="tab w-0"
      />
      <div className="tab-content pt-4">
        <SwatchPalette onChange={onChange} />
      </div>
    </div>
  );
}
