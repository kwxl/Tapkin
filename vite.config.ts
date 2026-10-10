import { defineConfig } from "vite";

export default defineConfig({
  root: "src",
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    // macOS 11's original WKWebView uses Safari 14. Keep generated assets
    // compatible with it instead of Vite's newer default browser baseline.
    target: ["safari14", "chrome87", "edge87", "firefox78"],
    cssTarget: ["safari14", "chrome87", "edge87", "firefox78"],
  },
});
