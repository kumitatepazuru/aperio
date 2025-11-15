import type { FrameLayerStructure } from "native";

const Frame = class {
  port?: MessagePort;

  init() {
    return new Promise((resolve) => {
      const listenerFunc = (event: MessageEvent) => {
        if (event.data.type !== "frame-port") return;

        const port: MessagePort = event.ports[0];
        this.port = port;

        port.start();
        // event listenerを削除
        window.removeEventListener("message", listenerFunc);
        resolve(null);
      };

      window.addEventListener("message", listenerFunc);

      window.frame.sendPort();
    });
  }

  async get(
    frameCount: number,
    frameStruct: FrameLayerStructure[]
  ): Promise<ArrayBuffer> {
    if (!this.port) {
      await this.init();
    }

    return new Promise((resolve) => {
      this.port?.addEventListener(
        "message",
        (e) => {
          resolve(e.data);
        },
        { once: true }
      );

      window.frame.getFrameBuf(frameCount, frameStruct);
    });
  }
};

export default Frame;
