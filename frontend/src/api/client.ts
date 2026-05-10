import type { ApiResponse } from "../types/api";

export type ApiClient = ReturnType<typeof createApiClient>;

const REQUEST_TIMEOUT_MS = 15_000;

export function createApiClient(token?: string | null) {
  async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const headers: Record<string, string> = { Accept: "application/json" };
    if (token) {
      headers.Authorization = `Bearer ${token}`;
    }
    if (body !== undefined) {
      headers["Content-Type"] = "application/json";
    }

    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);

    try {
      const response = await fetch(path, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: controller.signal
      });
      const payload = (await response.json().catch(() => ({}))) as ApiResponse<T>;

      if (!response.ok) {
        const details = payload.error?.details
          ?.map((detail) => `${detail.field} ${detail.message}`)
          .join(", ");
        throw new Error(details || payload.error?.message || `HTTP ${response.status}`);
      }

      return payload.data as T;
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") {
        throw new Error("Request timed out");
      }
      throw error;
    } finally {
      window.clearTimeout(timeout);
    }
  }

  return {
    get: <T = unknown>(path: string) => request<T>("GET", path),
    post: <T = unknown>(path: string, body?: unknown) => request<T>("POST", path, body),
    put: <T = unknown>(path: string, body?: unknown) => request<T>("PUT", path, body),
    delete: <T = unknown>(path: string) => request<T>("DELETE", path)
  };
}
