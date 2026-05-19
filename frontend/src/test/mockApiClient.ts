import { vi } from "vitest";

import type { ApiClient } from "../api/client";

type RouteValue = unknown | ((body?: unknown) => unknown | Promise<unknown>);

export type ApiCall = {
  method: "GET" | "POST" | "PUT" | "DELETE";
  path: string;
  body?: unknown;
};

export function createMockApiClient(routes: Record<string, RouteValue>) {
  const calls: ApiCall[] = [];

  async function resolve(method: ApiCall["method"], path: string, body?: unknown) {
    calls.push({ method, path, body });
    const route = routes[`${method} ${path}`] ?? routes[path];

    if (route === undefined) {
      throw new Error(`Unhandled ${method} ${path}`);
    }

    return typeof route === "function" ? route(body) : route;
  }

  const client = {
    get: vi.fn((path: string) => resolve("GET", path)),
    post: vi.fn((path: string, body?: unknown) => resolve("POST", path, body)),
    put: vi.fn((path: string, body?: unknown) => resolve("PUT", path, body)),
    delete: vi.fn((path: string) => resolve("DELETE", path))
  } as unknown as ApiClient;

  return { client, calls };
}
