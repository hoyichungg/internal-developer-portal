import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import App from "./App";
import type { MeResponse } from "./types/api";
import { renderWithProviders } from "./test/render";

vi.mock("./pages/dashboard/DashboardView", () => ({
  DashboardView: ({ client }: { client: { get: (path: string) => Promise<unknown> } }) => (
    <div>
      <span>Dashboard page</span>
      <button type="button" onClick={() => void client.get("/page-request").catch(() => undefined)}>
        Run page request
      </button>
    </div>
  )
}));

vi.mock("./pages/catalog/CatalogView", () => ({
  CatalogView: () => <div>Catalog page</div>
}));

vi.mock("./pages/audit/AuditView", () => ({
  AuditView: () => <div>Audit page</div>
}));

vi.mock("./pages/connectors/ConnectorsView", () => ({
  ConnectorsView: () => <div>Connectors page</div>
}));

vi.mock("./pages/services/ServiceOverviewView", () => ({
  ServiceOverviewView: () => <div>Service page</div>
}));

vi.mock("./pages/records/WorkCardDetailView", () => ({
  WorkCardDetailView: () => <div>Work card page</div>
}));

vi.mock("./pages/records/NotificationDetailView", () => ({
  NotificationDetailView: () => <div>Notification page</div>
}));

describe("App session lifecycle", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    window.localStorage.clear();
    window.history.replaceState(null, "", "/#dashboard");
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("clears a saved token when /me explicitly returns 401", async () => {
    window.localStorage.setItem("idp_token", "expired-token");
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      response({ error: { code: "unauthorized", message: "Session expired" } }, 401)
    );

    renderWithProviders(<App />);

    expect(await screen.findByLabelText(/Username/)).toHaveValue("");
    expect(window.localStorage.getItem("idp_token")).toBeNull();
  });

  it("keeps the token after a network failure and restores the session on retry", async () => {
    window.localStorage.setItem("idp_token", "saved-token");
    vi.spyOn(globalThis, "fetch")
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockResolvedValueOnce(response({ data: meResponse() }));

    renderWithProviders(<App />);

    expect(await screen.findByRole("heading", { name: "Session check unavailable" })).toBeVisible();
    expect(window.localStorage.getItem("idp_token")).toBe("saved-token");

    await userEvent.click(screen.getByRole("button", { name: "Retry" }));

    expect(await screen.findByText("Dashboard page")).toBeVisible();
    expect(window.localStorage.getItem("idp_token")).toBe("saved-token");
  });

  it("shows access denied without discarding the session", async () => {
    window.history.replaceState(null, "", "/#audit");
    window.localStorage.setItem("idp_token", "member-token");
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      response({ data: meResponse({ view_audit: false }) })
    );

    renderWithProviders(<App />);

    expect(await screen.findByText("You cannot open this view")).toBeVisible();
    expect(window.localStorage.getItem("idp_token")).toBe("member-token");
  });

  it("does not sign out when a page request returns 403", async () => {
    window.localStorage.setItem("idp_token", "member-token");
    vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(response({ data: meResponse() }))
      .mockResolvedValueOnce(response({ error: { message: "Forbidden" } }, 403));

    renderWithProviders(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "Run page request" }));

    await waitFor(() => expect(globalThis.fetch).toHaveBeenCalledTimes(2));
    expect(screen.getByText("Dashboard page")).toBeVisible();
    expect(window.localStorage.getItem("idp_token")).toBe("member-token");
  });

  it("signs out when any authenticated page request returns 401", async () => {
    window.localStorage.setItem("idp_token", "member-token");
    vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(response({ data: meResponse() }))
      .mockResolvedValueOnce(response({ error: { message: "Expired" } }, 401));

    renderWithProviders(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "Run page request" }));

    expect(await screen.findByLabelText(/Username/)).toBeVisible();
    expect(window.localStorage.getItem("idp_token")).toBeNull();
  });

  it("returns to the originally requested route after login", async () => {
    window.history.replaceState(null, "", "/#catalog");
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      if (String(input) === "/login") {
        return response({
          data: {
            token: "new-token",
            token_type: "Bearer",
            expires_at: "2099-01-01T00:00:00"
          }
        });
      }
      return response({ data: meResponse() });
    });

    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText(/Username/), "portal-user");
    await user.type(screen.getByLabelText(/Password/), "correct-password");
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    expect(await screen.findByText("Catalog page")).toBeVisible();
    expect(window.location.hash).toBe("#catalog");
    expect(window.localStorage.getItem("idp_token")).toBe("new-token");
    expect(window.localStorage.getItem("idp_token_expires_at")).toBe("2099-01-01T00:00:00");
  });

  it("expires a locally known session before making an API request", async () => {
    window.localStorage.setItem("idp_token", "expired-token");
    window.localStorage.setItem("idp_token_expires_at", "2000-01-01T00:00:00");
    const fetchMock = vi.spyOn(globalThis, "fetch");

    renderWithProviders(<App />);

    expect(await screen.findByLabelText(/Username/)).toBeVisible();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(window.localStorage.getItem("idp_token")).toBeNull();
  });

  it("syncs logout from another browser tab", async () => {
    window.localStorage.setItem("idp_token", "member-token");
    vi.spyOn(globalThis, "fetch").mockResolvedValue(response({ data: meResponse() }));

    renderWithProviders(<App />);
    expect(await screen.findByText("Dashboard page")).toBeVisible();

    window.localStorage.removeItem("idp_token");
    fireEvent(
      window,
      new StorageEvent("storage", {
        key: "idp_token",
        oldValue: "member-token",
        newValue: null
      })
    );

    expect(await screen.findByLabelText(/Username/)).toBeVisible();
  });
});

function meResponse(capabilities: Partial<MeResponse["capabilities"]> = {}): MeResponse {
  return {
    id: 7,
    username: "portal-user",
    roles: ["member"],
    expires_at: "2099-01-01T00:00:00",
    capabilities: {
      manage_connectors: false,
      view_audit: false,
      manage_maintainers: false,
      view_user_directory: false,
      ...capabilities
    },
    maintainer_access: []
  };
}

function response(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: {
      get: (name: string) =>
        name.toLowerCase() === "content-type" ? "application/json" : null
    },
    json: vi.fn().mockResolvedValue(body)
  } as unknown as Response;
}
