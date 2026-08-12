import { resolve } from "path";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed dev port and relative asset paths in production.
// Two entry points: the main window (index.html) and the recording overlay
// (overlay.html), each mounted as its own Tauri webview.
export default defineConfig({
  plugins: [svelte()],
  base: "./",
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    target: "safari15",
    outDir: "dist",
    // Never inline assets as data: URIs. The app's CSP is `default-src 'self'`
    // with no font-src, so a data: font is blocked outright — Vite's default
    // 4KB inline threshold silently swallowed four small woff2 subsets.
    assetsInlineLimit: 0,
    rollupOptions: {
      input: {
        main: resolve(import.meta.dirname, "index.html"),
        overlay: resolve(import.meta.dirname, "overlay.html"),
      },
    },
  },
});
