import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";
import tailwindcss from "@tailwindcss/vite";
import { rmSync } from "node:fs";

// root は apps/。各アプリ(ui / osr / dialog)の index.html が /<app>/ で配信される。
// 静的ファイルは apps/public/ (vite デフォルトの publicDir)から配信される。
const root = __dirname;
const outDir = resolve(__dirname, "../dist/app");

// vite が出力するもの。moon の ui:build の outputs と一致させること。
const viteOutputs = ["ui", "osr", "dialog", "assets", "vite.svg"];

// outDir は napi (.node / dll / native/) や rolldown (main.js / preload.js) と同居するため、
// emptyOutDir で outDir ごと空にすると他タスクの成果物まで消える。vite の出力だけを消す。
const cleanViteOutputs = () => ({
  name: "clean-vite-outputs",
  apply: "build" as const,
  buildStart() {
    for (const name of viteOutputs) {
      rmSync(resolve(outDir, name), { recursive: true, force: true });
    }
  },
});

// https://vite.dev/config/
export default defineConfig({
  base: "./",
  root,
  build: {
    outDir,
    emptyOutDir: false,
    rolldownOptions: {
      input: {
        ui: resolve(root, "ui", "index.html"),
        osr: resolve(root, "osr", "index.html"),
        dialog: resolve(root, "dialog", "index.html"),
      },
    },
  },
  plugins: [
    cleanViteOutputs(),
    react({
      babel: {
        plugins: [["babel-plugin-react-compiler"]],
      },
    }),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      "@": resolve(__dirname, "ui"),
      native: resolve(__dirname, "../dist/app/native/index.d.ts"),
    },
  },
});
