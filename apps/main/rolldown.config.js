import { defineConfig } from "rolldown";
import { esmExternalRequirePlugin } from "rolldown/plugins";
import { resolve } from "node:path";

const here = import.meta.dirname;

// 出力先は vite と同じ dist/app/ (main.js と preload.js は同じディレクトリに置く。
// napi が生成する .node ローダも bundle 後の main.js の隣にある .node を参照する)。
const outDir = resolve(here, "../../dist/app");

const common = {
  platform: "node",
  tsconfig: resolve(here, "tsconfig.json"),
  plugins: [
    esmExternalRequirePlugin({
      external: [/^node:/, "electron"],
    }),
  ],
};

// main と preload は別々にバンドルする。1つの設定にまとめると共通チャンクが生成され、
// preload が相対 import を持つことになるため。
export default defineConfig([
  {
    ...common,
    input: { main: resolve(here, "main.ts") },
    output: { dir: outDir, format: "esm", sourcemap: true },
  },
  {
    ...common,
    input: { preload: resolve(here, "../preload/main.ts") },
    output: { dir: outDir, format: "esm", sourcemap: true },
  },
]);
