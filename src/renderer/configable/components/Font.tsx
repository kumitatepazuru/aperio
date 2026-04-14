import { useRef, useState } from "react";
import { v4 as uuidv4 } from "uuid";
import { LuPencil } from "react-icons/lu";
import {
  FloatingBase,
  Reference,
  Floating,
  type FloatingHandle,
} from "@shared/Floating";
import type { FontValue } from "../utils";

const WEIGHT_NAMES: Record<number, string> = {
  100: "Thin",
  200: "Extra Light",
  300: "Light",
  400: "Normal",
  500: "Medium",
  600: "Semi Bold",
  700: "Bold",
  800: "Extra Bold",
  900: "Black",
};

function closestWeight(weights: number[], target: number): number {
  return weights.reduce((prev, curr) =>
    Math.abs(curr - target) < Math.abs(prev - target) ? curr : prev,
  );
}

type Props = {
  value: FontValue;
  onChange: (value: FontValue) => void;
};

const Font = ({ value, onChange }: Props) => {
  const floatingRef = useRef<FloatingHandle>(null);
  const [fontsList, setFontsList] = useState<Record<string, number[]> | null>(
    null,
  );
  const [radioName, setRadioName] = useState(`font-selection-${uuidv4()}`);

  const handleFloatingChange = (isOpen: boolean) => {
    if (isOpen) {
      setRadioName(`font-selection-${uuidv4()}`);
      if (!fontsList) {
        window.main.getFontsList().then(setFontsList);
      }
    }
  };

  const selectFont = (family: string, weight: number) => {
    onChange({ family, weight });
    floatingRef.current?.close();
  };

  const previewStyle = value.family
    ? { fontFamily: value.family, fontWeight: value.weight }
    : undefined;

  return (
    <FloatingBase ref={floatingRef} onChange={handleFloatingChange}>
      <Reference className="flex items-center gap-2 min-w-0">
        <span className="flex-1 text-sm truncate min-w-0" style={previewStyle}>
          {value.family ?? "(なし)"}{" "}
          {WEIGHT_NAMES[value.weight] ?? value.weight}
        </span>
        <button
          type="button"
          className="btn btn-square btn-sm shrink-0"
          onClick={() => floatingRef.current?.switch()}
        >
          <LuPencil />
        </button>
      </Reference>
      <Floating className="max-h-[min(80vh,50rem)] overflow-y-auto overflow-x-hidden bg-base-300">
        {fontsList ? (
          <div className="join join-vertical">
            {Object.entries(fontsList).map(([family, weights]) => {
              const sorted = [...weights].sort((a, b) => a - b);
              const displayWeight = sorted.includes(400)
                ? 400
                : closestWeight(sorted, 400);
              const isSelected = value.family === family;

              if (sorted.length <= 1) {
                const w = sorted[0];
                return (
                  <button
                    key={family}
                    type="button"
                    className={`btn join-item btn-sm text-left justify-start ${isSelected ? " btn-primary" : ""}`}
                    style={{ fontFamily: family, fontWeight: displayWeight }}
                    onClick={() => selectFont(family, w)}
                  >
                    {family}
                  </button>
                );
              }

              return (
                <div
                  key={family}
                  className="collapse collapse-arrow join-item border-base-300 border"
                >
                  <input type="radio" name={radioName} />
                  <div
                    className={`collapse-title text-sm ${isSelected ? " bg-primary text-primary-content" : "bg-base-200"}`}
                    style={{ fontFamily: family, fontWeight: displayWeight }}
                  >
                    {family}
                  </div>
                  <div className="collapse-content p-0">
                    <div className="join join-vertical w-full pb-5">
                      {sorted.map((w) => {
                        const isWeightSelected =
                          isSelected && value.weight === w;
                        return (
                          <button
                            key={w}
                            type="button"
                            className={`btn join-item btn-sm text-left justify-start ${isWeightSelected ? " btn-primary" : ""}`}
                            onClick={() => selectFont(family, w)}
                          >
                            <span style={{ fontWeight: w }}>
                              {WEIGHT_NAMES[w] ?? String(w)}
                            </span>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="flex justify-center p-4">
            <span className="loading loading-spinner" />
          </div>
        )}
      </Floating>
    </FloatingBase>
  );
};

export default Font;
