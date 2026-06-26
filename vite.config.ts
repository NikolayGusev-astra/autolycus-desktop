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
