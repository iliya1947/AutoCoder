import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(async () => ({
  plugins: [
    react(),
    {
      name: "disable-monaco-loader-cdn-fallback",
      enforce: "pre",
      transform(code, id) {
        if (!id.includes("@monaco-editor/loader") || !id.includes("config")) return;
        return code.replace(/https:\/\/cdn\.jsdelivr\.net\/npm\/monaco-editor@[^/]+\/min\/vs/g, "/monaco-editor/vs");
      },
    },
  ],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring tauri errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: true,
    watch: {
      // Cargo rewrites and locks binaries in this directory while `tauri dev`
      // is running. Watching them is unnecessary and causes EBUSY on Windows.
      ignored: ["**/src-tauri/target/**"],
    },
  },
}));
