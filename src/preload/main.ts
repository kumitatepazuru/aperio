import { contextBridge } from "electron";
import { plus100 } from "../native/index.js";

// native.plus100
contextBridge.exposeInMainWorld("native", {
  plus100: (input: number): number => {
    return plus100(input);
  },
});

// 例: renderer 側で window.native.plus100() が使える
declare global {
  interface Window {
    native: {
      plus100: (input: number) => number;
    };
  }
}
