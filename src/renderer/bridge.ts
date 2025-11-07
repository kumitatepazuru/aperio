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

      window.frame.init();
    });
  }

  async get(frameCount: number, frameStruct: FrameLayerStructure[]): Promise<Uint8Array<ArrayBufferLike>> {
    if (!this.port) {
      await this.init();
    }

    return new Promise((resolve) => {
      this.port?.addEventListener(
        "message",
        (e) => {
          const data = e.data as Uint8Array<ArrayBufferLike>;
          resolve(data);
        },
        { once: true }
      );

      window.frame.getFrame(frameCount, frameStruct);
    });
  }
};

export default Frame;
