import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "0.0.0.0",
    // Prevent Vite from scanning the Rust target/ build artefacts (they contain
    // dozens of generated tauri-codegen-assets/*.html files that crash esbuild's
    // dependency scanner with "The service was stopped").
    watch: {
      ignored: ["**/src-tauri/target/**"],
    },
  },
  // Only scan index.html for dependency pre-bundling — the default scanner
  // walks all *.html in the project, including src-tauri/target/ artefacts.
  optimizeDeps: {
    entries: ["index.html"],
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks: {
          "react-vendor": ["react", "react-dom"],
          "markdown": ["react-markdown", "remark-gfm", "react-syntax-highlighter"],
          "icons": ["lucide-react"],
          "state": ["zustand"],
        },
      },
    },
    chunkSizeWarningLimit: 600,
  },
});
