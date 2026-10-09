import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    // The API is served separately in development; proxying keeps the app origin-relative
    // so the same build works in production behind one host.
    proxy: { "/api": { target: "http://localhost:8000", changeOrigin: true } },
  },
});
