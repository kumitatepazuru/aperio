import { useEffect, useState, useRef } from "react";

const App = () => {
  const [frameCount, setFrameCount] = useState(0);
  const [currentFPS, setCurrentFPS] = useState(0);
  const requestIdRef = useRef<number>(null);

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

    requestIdRef.current = requestAnimationFrame(animate);

    return () => {
      if (requestIdRef.current) {
        cancelAnimationFrame(requestIdRef.current);
      }
    };
  }, []);

  return (
    <div className="flex flex-col gap-3 items-center justify-center w-screen h-screen">
      <p className="text-2xl">
        描画システムが正しく動作していません。この画面は通常見えるべきではありません。
        <br />
        The rendering system is not functioning correctly. This screen should
        not normally be visible.
      </p>
      <div className="relative border p-4">
        <div className="absolute -top-3 w-full left-0 text-center">
          <span className="bg-white px-2 text-sm">System information</span>
        </div>
        <div className="flex gap-3">
          <div>Current FPS: {currentFPS}</div>
          <div>Frame Count: {frameCount}</div>
        </div>
      </div>
    </div>
  );
};

export default App;
