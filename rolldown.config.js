import { defineConfig } from "rolldown";
import { esmExternalRequirePlugin } from "rolldown/experimental";

export default defineConfig({
  platform: "node",
  input: {
    main: "src/main/main.ts",
    preload: "src/preload/main.ts",
  },
  output: {
    dir: "dist/",
    format: "esm",
    sourcemap: true,
  },
  tsconfig: "tsconfig.main.json",
  plugins: [
    esmExternalRequirePlugin({
      external: [/^node:/, "electron"],
    }),
  ],
});
