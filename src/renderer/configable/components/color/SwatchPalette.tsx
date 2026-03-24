import type { ColorValue } from "../../types";
import ColorSwatch from "./ColorSwatch";
import {
  BLACK,
  oklchToColorValue,
  PALETTE_COLORS,
  SHADES,
  WHITE,
} from "./palette";

type Props = {
  onChange: (value: ColorValue) => void;
};

export default function SwatchPalette({ onChange }: Props) {
  return (
    <div className="flex flex-col gap-1">
      {/* Header row: shade labels */}
      <div className="flex items-center">
        <div className="w-12 shrink-0" />
        {SHADES.map((shade) => (
          <div
            key={shade}
            className="w-5 shrink-0 text-center opacity-40"
            style={{ fontSize: "9px" }}
          >
            {shade}
          </div>
        ))}
      </div>

      {/* Color rows */}
      <div className="overflow-y-auto max-h-96 flex flex-col gap-px">
        {PALETTE_COLORS.map(({ name, shades }) => (
          <div key={name} className="flex items-center">
            <div className="w-12 shrink-0 text-right text-xs opacity-60 pr-1.5">
              {name}
            </div>
            {SHADES.map((shade) => {
              const cv = oklchToColorValue(shades[shade]);
              return (
                <ColorSwatch
                  key={shade}
                  color={cv}
                  onClick={() => onChange(cv)}
                  containAlpha={false}
                />
              );
            })}
          </div>
        ))}
      </div>

      {/* Black & White section */}
      <div className="flex items-center gap-2 border-t border-base-content/10 pt-1.5">
        <span className="text-xs opacity-60">black / white</span>
        <ColorSwatch
          color={BLACK}
          onClick={() => onChange(BLACK)}
          containAlpha={false}
        />
        <ColorSwatch
          color={WHITE}
          onClick={() => onChange(WHITE)}
          containAlpha={false}
        />
      </div>
    </div>
  );
}
