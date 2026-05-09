import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/audit-logs": "http://127.0.0.1:8000",
      "/connectors": "http://127.0.0.1:8000",
      "/dashboard": "http://127.0.0.1:8000",
      "/health": "http://127.0.0.1:8000",
      "/login": "http://127.0.0.1:8000",
      "/logout": "http://127.0.0.1:8000",
      "/me": "http://127.0.0.1:8000",
      "/packages": "http://127.0.0.1:8000",
      "/services": "http://127.0.0.1:8000",
      "/work-cards": "http://127.0.0.1:8000"
    }
  }
});
