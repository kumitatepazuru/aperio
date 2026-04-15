import { useCallback, useRef, useState } from "react";
import { v4 as uuidv4 } from "uuid";
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
  const [filter, setFilter] = useState("");

  const scrollToSelected = useCallback((node: HTMLElement | null) => {
    if (node) {
      requestAnimationFrame(() => {
        node.scrollIntoView({ block: "nearest" });
      });
    }
  }, []);

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
      <Reference className="min-w-0">
        <button
          type="button"
          className="btn btn-sm btn-block"
          onClick={() => floatingRef.current?.switch()}
        >
          <span style={previewStyle} className="truncate">
            {value.family ?? "(なし)"}{" "}
            {WEIGHT_NAMES[value.weight] ?? value.weight}
          </span>
        </button>
      </Reference>
      <Floating className="h-[min(80vh,50rem)] overflow-y-auto bg-base-300">
        {fontsList ? (
          <div className="join join-vertical">
            <input
              type="text"
              placeholder="フォントを検索..."
              className="input input-sm w-full mb-2 sticky top-0 z-10"
              value={filter}
              onChange={(e) => setFilter(e.target.value.toLowerCase())}
            />
            {Object.entries(fontsList).map(([family, weights]) => {
              if (filter && !family.toLowerCase().includes(filter)) {
                return null;
              }
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
                    ref={isSelected ? scrollToSelected : undefined}
                    type="button"
                    className={`btn join-item text-left justify-start ${isSelected ? " btn-primary" : ""}`}
                    style={{ fontFamily: family, fontWeight: displayWeight }}
                    onClick={() => selectFont(family, w)}
                  >
                    {family} {WEIGHT_NAMES[w] ?? String(w)}
                  </button>
                );
              }

              return (
                <div
                  key={family}
                  className="collapse collapse-arrow join-item border-base-300 border"
                >
                  <input type="radio" name={radioName} defaultChecked={isSelected} />
                  <div
                    ref={isSelected ? scrollToSelected : undefined}
                    className={`collapse-title text-sm h-10 flex items-center ${isSelected ? " bg-primary text-primary-content" : "bg-base-200"}`}
                    style={{ fontFamily: family, fontWeight: displayWeight }}
                  >
                    {family}
                  </div>
                  <div className="collapse-content p-0">
                    <div className="join join-vertical w-full">
                      {sorted.map((w) => {
                        const isWeightSelected =
                          isSelected && value.weight === w;
                        return (
                          <button
                            key={w}
                            type="button"
                            className={`btn join-item btn-sm pl-5 text-left justify-start ${isWeightSelected ? " btn-primary" : ""}`}
                            onClick={() => selectFont(family, w)}
                          >
                            <span style={{ fontWeight: w }}>
                              {w} {WEIGHT_NAMES[w] ?? ""}
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
