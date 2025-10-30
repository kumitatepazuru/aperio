import { PlManager } from "../native";

declare global {
  interface Window {
    native: {
      getPlManager: () => Promise<PlManager>;
    };
    path: {
      getPath: (name: "userData" | "temp" | "exe") => Promise<string>;
      getResources: () => Promise<string>;
    };
  }
}
