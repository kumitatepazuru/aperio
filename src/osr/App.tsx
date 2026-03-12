import { useEffect, useState, useRef } from "react";
import type { SyncedStoreState } from "../preload/main";

const App = () => {
  const [frameCount, setFrameCount] = useState(0);
  const [currentFPS, setCurrentFPS] = useState(0);
  const [storeState, setStoreState] = useState<SyncedStoreState | null>(null);
  const requestIdRef = useRef<number>(null);

  useEffect(() => {
    window.storeSync?.onStoreState((state) => {
      setStoreState(state);
    });
  }, []);

  useEffect(() => {
    let frameCounter = 0;
    let lastFpsUpdateTime = performance.now();

    const animate = (time: number) => {
      frameCounter++;

      // FPS表示は0.5秒ごとに更新（負荷軽減）
      const timeSinceLastFpsUpdate = time - lastFpsUpdateTime;
      if (timeSinceLastFpsUpdate >= 500) {
        const fps = Math.round((frameCounter / timeSinceLastFpsUpdate) * 1000);
        setCurrentFPS(fps);
        frameCounter = 0;
        lastFpsUpdateTime = time;
      }

      // フレームカウントは0.1秒ごとに更新（表示用）
      setFrameCount((prev) => prev + 1);

      requestIdRef.current = requestAnimationFrame(animate);
    };

    if (storeState?.viewerState.state === "playing") {
      requestIdRef.current = requestAnimationFrame(animate);
    }

    return () => {
      if (requestIdRef.current) {
        cancelAnimationFrame(requestIdRef.current);
      }
    };
  }, [storeState?.viewerState.state]);

  return (
    <div
      className="flex flex-col items-center justify-center w-screen h-screen"
      style={{
        backgroundImage:
          "conic-gradient(#000 90deg, #ff69b4 90deg 180deg, #000 180deg 270deg, #ff69b4 270deg)",
        backgroundSize: "80px 80px",
      }}
    >
      <div className="flex flex-col gap-3 items-center bg-white p-5">
        <p className="text-2xl">
          描画システムが正しく動作していません。この画面は通常見えるべきではありません。
          <br />
          The rendering system is not functioning correctly. This screen should
          not normally be visible.
        </p>
        <div className="flex gap-5">
          <div className="relative border p-4">
            <div className="absolute -top-3 w-full left-0 text-center">
              <span className="bg-white px-2 text-sm">System information</span>
            </div>
            <div className="flex gap-1 flex-col font-mono">
              <div>
                <span className="font-bold">fps:</span> {currentFPS}
              </div>
              <div>
                <span className="font-bold">Frame Count:</span> {frameCount}
              </div>
            </div>
          </div>
          {storeState && (
            <div className="relative border p-4">
              <div className="absolute -top-3 w-full left-0 text-center">
                <span className="bg-white px-2 text-sm">Renderer store</span>
              </div>
              <div className="flex flex-col gap-1 font-mono">
                <div>
                  <span className="font-bold">fps:</span> {storeState.fps}
                </div>
                <div>
                  <span className="font-bold">state:</span>{" "}
                  {storeState.viewerState.state}
                </div>
                <div>
                  <span className="font-bold">beginFrame:</span>{" "}
                  {storeState.viewerState.beginFrame}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default App;
