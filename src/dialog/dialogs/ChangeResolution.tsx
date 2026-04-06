import useStore from "@shared/store";
import { useState } from "react";
import { useShallow } from "zustand/shallow";

// TODO: プリセットを変更可能に
const Presets = [
  { label: "4K", width: 3840, height: 2160 },
  { label: "1440p", width: 2560, height: 1440 },
  { label: "1080p", width: 1920, height: 1080 },
  { label: "720p", width: 1280, height: 720 },
  { label: "480p", width: 720, height: 480 },
  { label: "VGA", width: 640, height: 480 },
  { label: "4K(縦)", width: 2160, height: 3840 },
  { label: "1440p(縦)", width: 1440, height: 2560 },
  { label: "1080p(縦)", width: 1080, height: 1920 },
  { label: "720p(縦)", width: 720, height: 1280 },
  { label: "480p(縦)", width: 480, height: 720 },
  { label: "VGA(縦)", width: 480, height: 640 },
];

// TODO: なんかのutilityに移す？
const gcd: (a: number, b: number) => number = (a, b) =>
  b == 0 ? a : gcd(b, a % b);

const ChangeResolution = () => {
  const { frameState, setFrameState } = useStore(
    useShallow((state) => ({
      frameState: state.frameState,
      setFrameState: state.setFrameState,
    })),
  );
  const [preset, setPreset] = useState("custom");
  const [width, setWidth] = useState(frameState.width);
  const [height, setHeight] = useState(frameState.height);

  const change = () => {
    setFrameState({ ...frameState, width, height });
    window.close();
  };

  return (
    <div className="flex items-center justify-center w-screen h-screen">
      <div className="flex flex-col gap-2">
        <h1 className="text-2xl font-bold mb-4">解像度の変更</h1>
        <select
          className="select"
          value={preset}
          onChange={(e) => {
            const [newWidth, newHeight] = e.target.value.split("x").map(Number);
            setWidth(newWidth);
            setHeight(newHeight);
            setPreset(e.target.value);
          }}
        >
          {Presets.map((preset) => {
            const { width, height, label } = preset;
            const sizeGcd = gcd(width, height);

            return (
              <option key={label} value={`${width}x${height}`}>
                {label} ({width}x{height}) - {width / sizeGcd}:
                {height / sizeGcd}
              </option>
            );
          })}
          <option value={"custom"}>カスタム...</option>
        </select>
        <table className="table table-sm">
          <thead>
            <tr>
              <th className="w-20"></th>
              <th>現在の値</th>
              <th className="w-30">変更後</th>
            </tr>
          </thead>
          <tbody>
            <tr className="text-base">
              <th>幅</th>
              <td>
                <span className="font-mono">{frameState.width}</span>
              </td>
              <td>
                <div className="flex flex-col gap-2">
                  <input
                    className="input input-sm"
                    type="number"
                    value={width}
                    onChange={(e) => {
                      setWidth(Number(e.target.value));
                      setPreset("custom");
                    }}
                  />
                </div>
              </td>
            </tr>
            <tr className="text-base">
              <th>
                <span>高さ</span>
              </th>
              <td>
                <span className="font-mono">{frameState.height}</span>
              </td>
              <td>
                <div className="flex flex-col gap-2">
                  <input
                    className="input input-sm"
                    type="number"
                    value={height}
                    onChange={(e) => {
                      setHeight(Number(e.target.value));
                      setPreset("custom");
                    }}
                  />
                </div>
              </td>
            </tr>
          </tbody>
        </table>
        <button className="btn btn-primary" onClick={change}>
          変更を保存
        </button>
      </div>
    </div>
  );
};

export default ChangeResolution;
