import { aperioPlugin } from "@aperio/plugin-sdk/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [aperioPlugin(), tailwindcss()],
});
