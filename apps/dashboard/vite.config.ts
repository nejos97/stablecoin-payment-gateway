import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    proxy: {
      // Trailing slash on purpose: "/api" alone would also swallow SPA
      // routes like /api-keys on full page loads.
      "/api/": "http://localhost:3000",
      "/healthz": "http://localhost:3000",
    },
  },
})
