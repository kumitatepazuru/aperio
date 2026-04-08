import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";
import tailwindcss from "@tailwindcss/vite";

const root = resolve(__dirname, "src");
const outDir = resolve(__dirname, "dist");

// https://vite.dev/config/
export default defineConfig({
  base: "./",
  root,
  build: {
    outDir,
    emptyOutDir: true,
    rolldownOptions: {
      input: {
        main: resolve(root, "renderer", "index.html"),
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
      "@": resolve(__dirname, "src/renderer"),
      "@shared": resolve(__dirname, "src/shared"),
      native: resolve(__dirname, "dist/native/index.d.ts"),
    },
  },
});
