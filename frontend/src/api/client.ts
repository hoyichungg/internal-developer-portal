import type { ApiErrorDetail, ApiResponse } from "../types/api";

export type ApiClient = ReturnType<typeof createApiClient>;

export type ApiErrorKind = "http" | "invalid_response" | "network" | "timeout";

type ApiErrorOptions = {
  kind: ApiErrorKind;
  status?: number;
  code?: string;
  details?: ApiErrorDetail[];
  cause?: unknown;
};

export type ApiClientOptions = {
  baseUrl?: string;
  timeoutMs?: number;
  onUnauthorized?: () => void;
  getSessionGeneration?: () => number;
};

const REQUEST_TIMEOUT_MS = 15_000;
const CONFIGURED_API_BASE_URL = import.meta.env.VITE_API_BASE_URL?.trim() || "";

export class ApiError extends Error {
  readonly kind: ApiErrorKind;
  readonly status?: number;
  readonly code?: string;
  readonly details?: ApiErrorDetail[];
  readonly cause?: unknown;

  constructor(message: string, options: ApiErrorOptions) {
    super(message);
    this.name = "ApiError";
    this.kind = options.kind;
    this.status = options.status;
    this.code = options.code;
    this.details = options.details;
    this.cause = options.cause;
  }
}

export function isApiError(error: unknown): error is ApiError {
  return error instanceof ApiError;
}

export function createApiClient(options: ApiClientOptions = {}) {
  const baseUrl = options.baseUrl ?? CONFIGURED_API_BASE_URL;
  const timeoutMs = options.timeoutMs ?? REQUEST_TIMEOUT_MS;
  const onUnauthorized = options.onUnauthorized;
  const getSessionGeneration = options.getSessionGeneration;

  async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const requestSessionGeneration = getSessionGeneration?.();
    const headers: Record<string, string> = { Accept: "application/json" };
    if (body !== undefined) {
      headers["Content-Type"] = "application/json";
    }
    if (!["GET", "HEAD", "OPTIONS"].includes(method)) {
      headers["X-IDP-CSRF"] = "1";
    }

    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), timeoutMs);

    try {
      const response = await fetch(resolveApiUrl(baseUrl, path), {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
        credentials: "include",
        signal: controller.signal
      });

      if (
        response.status === 401 &&
        (requestSessionGeneration === undefined ||
          getSessionGeneration?.() === requestSessionGeneration)
      ) {
        onUnauthorized?.();
      }

      if (response.status === 204) {
        return undefined as T;
      }

      const contentType = response.headers.get("content-type") || "";
      if (!isJsonContentType(contentType)) {
        if (!response.ok) {
          throw new ApiError(
            `HTTP ${response.status}: the server returned a non-JSON error response`,
            { kind: "http", status: response.status }
          );
        }

        throw new ApiError("Invalid API response: expected Content-Type application/json", {
          kind: "invalid_response",
          status: response.status
        });
      }

      let payload: unknown;
      try {
        payload = await response.json();
      } catch (error) {
        const kind = response.ok ? "invalid_response" : "http";
        throw new ApiError("Invalid API response: the response body is not valid JSON", {
          kind,
          status: response.status,
          cause: error
        });
      }

      if (!response.ok) {
        const apiPayload = isRecord(payload) ? (payload as ApiResponse<unknown>) : undefined;
        const details = normalizeErrorDetails(apiPayload?.error?.details);
        const code =
          typeof apiPayload?.error?.code === "string" ? apiPayload.error.code : undefined;
        const detailMessage = details.map((detail) => `${detail.field} ${detail.message}`).join(", ");
        throw new ApiError(
          detailMessage || apiPayload?.error?.message || `HTTP ${response.status}`,
          { kind: "http", status: response.status, code, details }
        );
      }

      if (!isRecord(payload) || !Object.prototype.hasOwnProperty.call(payload, "data")) {
        throw new ApiError(
          "Invalid API response: expected a JSON object with a data field",
          { kind: "invalid_response", status: response.status }
        );
      }

      return payload.data as T;
    } catch (error) {
      if (isApiError(error)) {
        throw error;
      }
      if (controller.signal.aborted || isAbortError(error)) {
        throw new ApiError(`Request timed out after ${timeoutMs} ms`, {
          kind: "timeout",
          cause: error
        });
      }
      throw new ApiError("Unable to reach the API. Check your network connection and try again.", {
        kind: "network",
        cause: error
      });
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

function resolveApiUrl(baseUrl: string, path: string): string {
  const normalizedBaseUrl = baseUrl.trim().replace(/\/+$/, "");

  if (!normalizedBaseUrl) {
    return path;
  }

  return `${normalizedBaseUrl}/${path.replace(/^\/+/, "")}`;
}

function isJsonContentType(contentType: string): boolean {
  return /^application\/(?:[\w.+-]+\+)?json(?:\s*;|\s*$)/i.test(contentType);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function normalizeErrorDetails(details: unknown): ApiErrorDetail[] {
  if (!Array.isArray(details)) {
    return [];
  }

  return details.filter(
    (detail): detail is ApiErrorDetail =>
      isRecord(detail) && typeof detail.field === "string" && typeof detail.message === "string"
  );
}

function isAbortError(error: unknown): boolean {
  return (
    (typeof DOMException !== "undefined" && error instanceof DOMException && error.name === "AbortError") ||
    (isRecord(error) && error.name === "AbortError")
  );
}
