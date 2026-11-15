import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: "127.0.0.1",
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    // macOS WKWebView on older systems is Safari 16; WebView2 is evergreen.
    target: ["safari16", "chrome110"],
    sourcemap: false,
  },
});
