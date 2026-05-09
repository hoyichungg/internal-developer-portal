export type ApiClient = ReturnType<typeof createApiClient>;

export function createApiClient(token?: string | null) {
  async function request(method: string, path: string, body?: unknown) {
    const headers: Record<string, string> = { Accept: "application/json" };
    if (token) {
      headers.Authorization = `Bearer ${token}`;
    }
    if (body !== undefined) {
      headers["Content-Type"] = "application/json";
    }

    const response = await fetch(path, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body)
    });
    const payload = await response.json().catch(() => ({}));

    if (!response.ok) {
      const details = payload.error?.details
        ?.map((detail: { field: string; message: string }) => `${detail.field} ${detail.message}`)
        .join(", ");
      throw new Error(details || payload.error?.message || `HTTP ${response.status}`);
    }

    return payload.data;
  }

  return {
    get: (path: string) => request("GET", path),
    post: (path: string, body?: unknown) => request("POST", path, body),
    put: (path: string, body?: unknown) => request("PUT", path, body),
    delete: (path: string) => request("DELETE", path)
  };
}
