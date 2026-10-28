import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";
import tailwindcss from "@tailwindcss/vite";

// root は apps/。各アプリ(ui / osr / dialog)の index.html が /<app>/ で配信される。
// 静的ファイルは apps/public/ (vite デフォルトの publicDir)から配信される。
const root = __dirname;
const outDir = resolve(__dirname, "../dist/app");

// https://vite.dev/config/
export default defineConfig({
  base: "./",
  root,
  build: {
    outDir,
    emptyOutDir: true,
    rolldownOptions: {
      input: {
        ui: resolve(root, "ui", "index.html"),
        osr: resolve(root, "osr", "index.html"),
        dialog: resolve(root, "dialog", "index.html"),
      },
    },
  },
  plugins: [
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
