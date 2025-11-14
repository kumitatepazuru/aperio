import type { LayerStructure } from "native";

let port: MessagePort;

const initFunc = new Promise((resolve) => {
  const listenerFunc = (event: MessageEvent) => {
    if (event.data.type !== "frame-port") return;

    port = event.ports[0];
    port.start();
    // event listenerを削除
    window.removeEventListener("message", listenerFunc);
    resolve(null);
  };

  window.addEventListener("message", listenerFunc);

  // initの終わりにportが送らてくるため、resolveされるとき即ちinitが完了しているとみなせる
  // そのため、awaitは必要ない
  window.native.init();
});

await initFunc;

const getFrame = async (
  frameCount: number,
  frameStruct: LayerStructure[]
): Promise<ArrayBuffer> => {
  if (!port) {
    await initFunc;
  }

  return new Promise((resolve) => {
    port?.addEventListener(
      "message",
      (e) => {
        resolve(e.data);
      },
      { once: true }
    );

    window.frame.getFrame(frameCount, frameStruct);
  });
};

export { getFrame };
