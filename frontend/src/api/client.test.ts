import { afterEach, describe, expect, it, vi } from "vitest";

import { ApiError, createApiClient } from "./client";

describe("createApiClient", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("uses the configured API base URL and sends JSON headers and authorization", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      response({ data: { id: 7 } })
    );
    const client = createApiClient("session-token", {
      baseUrl: "https://portal-api.example.test/",
      timeoutMs: 100
    });

    await expect(client.post<{ id: number }>("/connectors", { source: "graph" })).resolves.toEqual({
      id: 7
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "https://portal-api.example.test/connectors",
      expect.objectContaining({
        method: "POST",
        headers: {
          Accept: "application/json",
          Authorization: "Bearer session-token",
          "Content-Type": "application/json"
        },
        body: JSON.stringify({ source: "graph" })
      })
    );
  });

  it("preserves HTTP status and structured validation details in ApiError", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      response(
        {
          error: {
            code: "validation_failed",
            message: "Validation failed",
            details: [{ field: "config", message: "must be valid" }]
          }
        },
        422
      )
    );

    await expect(createApiClient().put("/connectors/source/config", {})).rejects.toMatchObject({
      name: "ApiError",
      kind: "http",
      status: 422,
      code: "validation_failed",
      message: "config must be valid",
      details: [{ field: "config", message: "must be valid" }]
    });
  });

  it.each([
    ["missing JSON content type", response({ data: [] }, 200, "text/html"), "expected Content-Type application/json"],
    ["missing data envelope", response({ items: [] }), "expected a JSON object with a data field"]
  ])("rejects a successful response with %s", async (_label, apiResponse, message) => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(apiResponse);

    await expect(createApiClient().get("/connectors")).rejects.toMatchObject({
      name: "ApiError",
      kind: "invalid_response",
      status: 200,
      message: expect.stringContaining(message)
    });
  });

  it("accepts an empty 204 response for delete operations", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(response(undefined, 204, null));

    await expect(createApiClient().delete("/connectors/source")).resolves.toBeUndefined();
  });

  it("classifies aborted requests as timeouts", async () => {
    vi.useFakeTimers();
    vi.spyOn(globalThis, "fetch").mockImplementation((_input, init) =>
      new Promise((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => {
          reject(new DOMException("Aborted", "AbortError"));
        });
      })
    );

    const request = createApiClient(null, { timeoutMs: 25 }).get("/slow");
    const assertion = expect(request).rejects.toMatchObject({
      name: "ApiError",
      kind: "timeout",
      message: "Request timed out after 25 ms"
    });
    await vi.advanceTimersByTimeAsync(25);
    await assertion;
  });

  it("classifies fetch failures as network errors", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new TypeError("Failed to fetch"));

    const error = await createApiClient().get("/health").catch((caught) => caught);

    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      kind: "network",
      status: undefined,
      message: "Unable to reach the API. Check your network connection and try again."
    });
  });

  it("notifies authenticated callers only for an explicit 401 response", async () => {
    const onUnauthorized = vi.fn();
    vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(response({ error: { message: "expired" } }, 401))
      .mockResolvedValueOnce(response({ error: { message: "forbidden" } }, 403));
    const client = createApiClient("session-token", { onUnauthorized });

    await expect(client.get("/expired")).rejects.toMatchObject({ status: 401 });
    expect(onUnauthorized).toHaveBeenCalledTimes(1);

    await expect(client.get("/forbidden")).rejects.toMatchObject({ status: 403 });
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
  });
});

function response(body: unknown, status = 200, contentType: string | null = "application/json"): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: {
      get: (name: string) => (name.toLowerCase() === "content-type" ? contentType : null)
    },
    json: vi.fn().mockResolvedValue(body)
  } as unknown as Response;
}
