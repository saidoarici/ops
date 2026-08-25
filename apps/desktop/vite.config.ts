import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri geliştirme sunucusu: sabit port, src-tauri izlenmez.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "safari15",
    sourcemap: false,
  },
});
