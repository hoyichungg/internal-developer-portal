export const API_PROXY_PREFIXES = [
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
] as const;

const DEFAULT_API_BASE_URL = "http://127.0.0.1:8000";

export function resolveApiProxyTarget(value?: string): string {
  const candidate = value?.trim();

  if (!candidate) {
    return DEFAULT_API_BASE_URL;
  }

  try {
    const url = new URL(candidate);
    if (url.protocol === "http:" || url.protocol === "https:") {
      return candidate.replace(/\/+$/, "");
    }
  } catch {
    // A relative browser base URL cannot be used as the development proxy target.
  }

  return DEFAULT_API_BASE_URL;
}

export function createApiProxy(target: string) {
  return Object.fromEntries(
    API_PROXY_PREFIXES.map((prefix) => [prefix, { target, changeOrigin: true }])
  );
}
