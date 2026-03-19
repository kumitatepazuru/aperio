import { useEffect, useState } from "react";

const App = () => {
  const [eventStackID, setEventStackID] = useState(0);

  useEffect(() => {
    const onEvent = (length: number) => {
      setEventStackID(length);
    };

    const unsubscribe = window.main.onEventStackChanged(onEvent);
    window.main.getEventStack().then(onEvent);

    return () => {
      unsubscribe();
    };
  }, []);

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
                <span className="font-bold">eventStack ID:</span>{" "}
                {eventStackID}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default App;
