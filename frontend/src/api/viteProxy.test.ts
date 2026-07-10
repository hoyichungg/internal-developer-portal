import { describe, expect, it } from "vitest";

import {
  API_PROXY_PREFIXES,
  createApiProxy,
  resolveApiProxyTarget
} from "./viteProxy";

describe("Vite API proxy", () => {
  it("covers every backend top-level API prefix used by the portal", () => {
    expect(API_PROXY_PREFIXES).toEqual([
      "/audit-logs",
      "/connectors",
      "/dashboard",
      "/health",
      "/livez",
      "/login",
      "/logout",
      "/maintainers",
      "/me",
      "/notifications",
      "/openapi.json",
      "/packages",
      "/readyz",
      "/services",
      "/users",
      "/work-cards"
    ]);
  });

  it("applies an absolute VITE_API_BASE_URL to every proxy entry", () => {
    const target = resolveApiProxyTarget("  https://api.example.test/  ");
    const proxy = createApiProxy(target);

    expect(target).toBe("https://api.example.test");
    for (const prefix of API_PROXY_PREFIXES) {
      expect(proxy[prefix]).toEqual({
        target: "https://api.example.test",
        changeOrigin: true
      });
    }
  });

  it("keeps the local backend fallback when the browser base URL is relative", () => {
    expect(resolveApiProxyTarget("/api")).toBe("http://127.0.0.1:8000");
  });
});
