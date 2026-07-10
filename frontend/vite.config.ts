import react from "@vitejs/plugin-react";
import { loadEnv } from "vite";
import { defineConfig } from "vitest/config";

import { createApiProxy, resolveApiProxyTarget } from "./src/api/viteProxy";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, ".", "");
  const proxyTarget = resolveApiProxyTarget(env.VITE_API_BASE_URL);

  return {
    plugins: [react()],
    test: {
      environment: "jsdom",
      setupFiles: "./src/test/setup.ts"
    },
    build: {
      rollupOptions: {
        output: {
          manualChunks(id) {
            if (id.includes("node_modules/@mantine/")) return "mantine";
            if (id.includes("node_modules/@tabler/icons-react")) return "icons";
            if (id.includes("node_modules/react") || id.includes("node_modules/scheduler")) {
              return "react";
            }
            return undefined;
          }
        }
      }
    },
    server: {
      proxy: createApiProxy(proxyTarget)
    }
  };
});
