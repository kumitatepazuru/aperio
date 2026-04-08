import type { LayerResult, LayerStructure } from "native";

type PendingRequest = {
  frameCount: number;
  width: number;
  height: number;
  frameStruct: LayerStructure[];
  resolve: (data: BufferReturn) => void;
  reject: (err: unknown) => void;
};

type BufferReturn = {
  frame: ArrayBuffer;
  frameResults: Record<string, LayerResult>;
};

const Frame = class {
  port?: MessagePort;
  // 現在 IPC リクエストが飛んでいるかどうか
  // port には同時に1つのリスナーしか登録しないことでレスポンスの取り違えを防ぐ
  private _inFlight = false;
  // 処理待ちの最新リクエスト（上書きされた古いものはdropされる）
  private _pending: PendingRequest | null = null;

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

  private _execute(req: PendingRequest) {
    this._inFlight = true;

    this.port!.addEventListener(
      "message",
      (e) => {
        if (!e.data.frame)
          throw new Error("Invalid response: missing frame data");
        if (!e.data.frameResults)
          throw new Error("Invalid response: missing frame results");

        req.resolve(e.data);
        this._inFlight = false;

        // 処理中に積まれた最新リクエストがあればすぐ実行
        if (this._pending) {
          const next = this._pending;
          this._pending = null;
          this._execute(next);
        }
      },
      { once: true },
    );

    window.frame.getFrameBuf(
      req.frameCount,
      req.width,
      req.height,
      req.frameStruct,
    );
  }

  async getBuf(
    frameCount: number,
    width: number,
    height: number,
    frameStruct: LayerStructure[],
  ): Promise<BufferReturn> {
    if (!this.port) {
      await this.init();
    }

    return new Promise((resolve, reject) => {
      if (this._inFlight) {
        // 処理中のリクエストがある場合、古い pending は drop して最新だけ保持
        if (this._pending) {
          this._pending.reject(new Error("dropped"));
        }
        this._pending = {
          frameCount,
          width,
          height,
          frameStruct,
          resolve,
          reject,
        };
      } else {
        this._execute({
          frameCount,
          width,
          height,
          frameStruct,
          resolve,
          reject,
        });
      }
    });
  }
};

export default Frame;
