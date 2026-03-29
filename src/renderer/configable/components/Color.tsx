import { FloatingBase, Reference, Floating } from "@shared/Floating";
import type { ColorValue } from "../utils";
import { checkerStyle, toColor } from "./color/helpers";
import ColorPickerPanel from "./color/ColorPickerPanel";

type Props = {
  value: ColorValue;
  useAlpha: boolean;
  onChange: (value: ColorValue) => void;
};

const Color = ({ value, useAlpha, onChange }: Props) => (
  <FloatingBase>
    <Reference click className="inline-flex items-center gap-2 cursor-pointer">
      <div className="w-12 h-6 rounded border border-base-content/30 relative overflow-hidden shrink-0">
        <div className="absolute inset-0" style={checkerStyle} />
        <div
          className="absolute inset-0"
          style={{
            background: useAlpha
              ? `rgba(${value[0] * 255} ${value[1] * 255} ${value[2] * 255} / ${value[3]})`
              : `rgb(${value[0] * 255} ${value[1] * 255} ${value[2] * 255})`,
          }}
        />
      </div>
      <span className="text-sm font-mono">
        {toColor(value).toString({ format: "hex" })}
      </span>
    </Reference>
    <Floating>
      <ColorPickerPanel value={value} useAlpha={useAlpha} onChange={onChange} />
    </Floating>
  </FloatingBase>
);

export default Color;
