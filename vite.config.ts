import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri  expects a fixed dev port and no base path
export default defineConfig({
  plugins: [vue()],
  base: "./",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // src-tauri 由 Rust 侧管理，别让 Vite 去 watch
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "esnext",
    minify: "esbuild",
    sourcemap: false,
  },
});
