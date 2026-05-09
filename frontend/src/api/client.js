export function createApiClient(token) {
  async function request(method, path, body) {
    const headers = { Accept: "application/json" };
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
        ?.map((detail) => `${detail.field} ${detail.message}`)
        .join(", ");
      throw new Error(details || payload.error?.message || `HTTP ${response.status}`);
    }

    return payload.data;
  }

  return {
    get: (path) => request("GET", path),
    post: (path, body) => request("POST", path, body),
    put: (path, body) => request("PUT", path, body),
    delete: (path) => request("DELETE", path)
  };
}
