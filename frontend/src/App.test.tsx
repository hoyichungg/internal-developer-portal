import { notifications } from "@mantine/notifications";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import App from "./App";
import { SESSION_EVENT_STORAGE_KEY } from "./sessionEvents";
import { renderWithProviders } from "./test/render";
import type { LoginResponse, MeResponse, PublicAuthConfig } from "./types/api";

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
  WorkCardDetailView: ({ onBack }: { onBack: () => void }) => (
    <div>
      <span>Work card page</span>
      <button type="button" onClick={onBack}>
        Back to work list
      </button>
    </div>
  )
}));

vi.mock("./pages/records/NotificationDetailView", () => ({
  NotificationDetailView: () => <div>Notification page</div>
}));

vi.mock("./pages/work/MyWorkView", () => ({
  MyWorkView: ({ searchParams }: { searchParams: URLSearchParams }) => (
    <div>My Work page {searchParams.toString()}</div>
  )
}));

describe("App cookie session lifecycle", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    window.localStorage.clear();
    window.history.replaceState(null, "", "/#dashboard");
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("restores an existing browser session through /me without an Authorization header", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(response({ data: meResponse() }));

    renderWithProviders(<App />);

    expect(await screen.findByText("Dashboard page")).toBeVisible();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0]?.[0]).toBe("/me");
    const request = fetchMock.mock.calls[0]?.[1];
    expect(request).toEqual(expect.objectContaining({ credentials: "include" }));
    expect(new Headers(request?.headers).has("Authorization")).toBe(false);
    expect(window.localStorage.getItem("idp_token")).toBeNull();
    expect(window.localStorage.getItem("idp_token_expires_at")).toBeNull();
  });

  it("restores a filtered My Work route and keeps its top-level navigation active", async () => {
    window.history.replaceState(null, "", "/#my-work?status=blocked&page=2");
    vi.spyOn(globalThis, "fetch").mockResolvedValue(response({ data: meResponse() }));

    renderWithProviders(<App />);

    expect(await screen.findByText("My Work page status=blocked&page=2")).toBeVisible();
    expect(screen.getByRole("link", { name: "My Work" })).toHaveAttribute(
      "aria-current",
      "page"
    );
  });

  it("returns from a work-card detail to the exact filtered My Work route", async () => {
    window.history.replaceState(
      null,
      "",
      "/#work-cards/42?from=my-work&status=blocked&due=overdue&page=2"
    );
    vi.spyOn(globalThis, "fetch").mockResolvedValue(response({ data: meResponse() }));

    renderWithProviders(<App />);
    expect(await screen.findByText("Work card page")).toBeVisible();
    expect(screen.getByRole("link", { name: "My Work" })).toHaveAttribute(
      "aria-current",
      "page"
    );

    await userEvent.click(screen.getByRole("button", { name: "Back to work list" }));

    expect(await screen.findByText("My Work page status=blocked&due=overdue&page=2")).toBeVisible();
    expect(window.location.hash).toBe("#my-work?status=blocked&due=overdue&page=2");
  });

  it("returns to login on /me 401 and purges legacy stored credentials", async () => {
    window.localStorage.setItem("idp_token", "expired-token");
    window.localStorage.setItem("idp_token_expires_at", "2099-01-01T00:00:00Z");
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) =>
      String(input) === "/auth/config"
        ? response({ data: authConfig() })
        : response({ error: { code: "unauthorized", message: "Session expired" } }, 401)
    );

    renderWithProviders(<App />);

    expect(await screen.findByLabelText(/Username/)).toHaveValue("");
    expect(window.localStorage.getItem("idp_token")).toBeNull();
    expect(window.localStorage.getItem("idp_token_expires_at")).toBeNull();
  });

  it("keeps the browser session untouched after a network failure and restores it on retry", async () => {
    vi.spyOn(globalThis, "fetch")
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockResolvedValueOnce(response({ data: meResponse() }));

    renderWithProviders(<App />);

    expect(await screen.findByRole("heading", { name: "Session check unavailable" })).toBeVisible();

    await userEvent.click(screen.getByRole("button", { name: "Retry" }));

    expect(await screen.findByText("Dashboard page")).toBeVisible();
  });

  it("does not claim to sign out when the logout request cannot reach the server", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockRejectedValue(new TypeError("Offline"));

    renderWithProviders(<App />);
    expect(await screen.findByRole("heading", { name: "Session check unavailable" })).toBeVisible();

    await userEvent.click(screen.getByRole("button", { name: "Sign out on this device" }));

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(screen.getByRole("heading", { name: "Session check unavailable" })).toBeVisible();
    expect(screen.queryByLabelText(/Username/)).not.toBeInTheDocument();
  });

  it("shows access denied without discarding the cookie session", async () => {
    window.history.replaceState(null, "", "/#audit");
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      response({ data: meResponse({ view_audit: false }) })
    );

    renderWithProviders(<App />);

    expect(await screen.findByText("You cannot open this view")).toBeVisible();
    expect(screen.queryByLabelText(/Username/)).not.toBeInTheDocument();
  });

  it("does not sign out when a page request returns 403", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(response({ data: meResponse() }))
      .mockResolvedValueOnce(response({ error: { message: "Forbidden" } }, 403));

    renderWithProviders(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "Run page request" }));

    await waitFor(() => expect(globalThis.fetch).toHaveBeenCalledTimes(2));
    expect(screen.getByText("Dashboard page")).toBeVisible();
    expect(screen.queryByLabelText(/Username/)).not.toBeInTheDocument();
    expect(setItem).not.toHaveBeenCalledWith(SESSION_EVENT_STORAGE_KEY, "signed-out");
  });

  it("returns to login when any authenticated page request returns 401", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(response({ data: meResponse() }))
      .mockResolvedValueOnce(response({ error: { message: "Expired" } }, 401))
      .mockResolvedValueOnce(response({ data: authConfig() }));

    renderWithProviders(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "Run page request" }));

    expect(await screen.findByLabelText(/Username/)).toBeVisible();
    expect(setItem).toHaveBeenCalledWith(SESSION_EVENT_STORAGE_KEY, "signed-out");
  });

  it("confirms logout with the server before clearing the browser session", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(response({ data: meResponse() }))
      .mockResolvedValueOnce(response(undefined, 204))
      .mockResolvedValueOnce(response({ data: authConfig() }));

    renderWithProviders(<App />);
    expect(await screen.findByText("Dashboard page")).toBeVisible();

    await userEvent.click(screen.getAllByRole("button", { name: "Sign out" })[0]);

    expect(await screen.findByLabelText(/Username/)).toBeVisible();
    expect(fetchMock.mock.calls.map(([input]) => String(input))).toEqual([
      "/me",
      "/logout",
      "/auth/config"
    ]);
    expect(fetchMock.mock.calls[1]?.[1]).toEqual(
      expect.objectContaining({ credentials: "include", method: "POST" })
    );
    expect(setItem).toHaveBeenCalledWith(SESSION_EVENT_STORAGE_KEY, "signed-out");
  });

  it("revokes every session only after explicit confirmation", async () => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(response({ data: meResponse() }))
      .mockResolvedValueOnce(response({ data: { revoked_sessions: 3 } }))
      .mockResolvedValueOnce(response({ data: authConfig() }));

    renderWithProviders(<App />);
    expect(await screen.findByText("Dashboard page")).toBeVisible();

    await userEvent.click(screen.getByRole("button", { name: "Sign out on all devices" }));

    expect(await screen.findByLabelText(/Username/)).toBeVisible();
    expect(window.confirm).toHaveBeenCalledWith(
      "Sign out this account on every device and browser?"
    );
    expect(fetchMock.mock.calls.map(([input]) => String(input))).toEqual([
      "/me",
      "/sessions/revoke-all",
      "/auth/config"
    ]);
  });

  it("returns to the requested route after cookie-only login", async () => {
    window.history.replaceState(null, "", "/#catalog");
    let sessionEstablished = false;
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const path = String(input);
      if (path === "/login") {
        sessionEstablished = true;
        return response({ data: loginResponse() });
      }
      if (path === "/auth/config") {
        return response({ data: authConfig() });
      }
      if (path === "/me" && sessionEstablished) {
        return response({ data: meResponse() });
      }
      return response({ error: { message: "Authentication is required" } }, 401);
    });

    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText(/Username/), "portal-user");
    await user.type(screen.getByLabelText(/Password/), "correct-password");
    await user.click(screen.getByRole("button", { name: "Sign in with password" }));

    expect(await screen.findByText("Catalog page")).toBeVisible();
    expect(window.location.hash).toBe("#catalog");
    expect(window.localStorage.getItem("idp_token")).toBeNull();
    expect(window.localStorage.getItem("idp_token_expires_at")).toBeNull();
    expect(fetchMock.mock.calls.map(([input]) => String(input))).toEqual([
      "/me",
      "/auth/config",
      "/login",
      "/me"
    ]);
    for (const [, request] of fetchMock.mock.calls) {
      expect(request).toEqual(expect.objectContaining({ credentials: "include" }));
      expect(new Headers(request?.headers).has("Authorization")).toBe(false);
    }
  });

  it("announces password sign-in only after /me confirms the cookie session", async () => {
    let resolveConfirmation: ((value: Response) => void) | undefined;
    const confirmation = new Promise<Response>((resolve) => {
      resolveConfirmation = resolve;
    });
    let sessionEstablished = false;
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const showNotification = vi.spyOn(notifications, "show");
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const path = String(input);
      if (path === "/auth/config") {
        return response({ data: authConfig() });
      }
      if (path === "/login") {
        sessionEstablished = true;
        return response({ data: loginResponse() });
      }
      if (path === "/me" && sessionEstablished) {
        return confirmation;
      }
      return response({ error: { message: "Authentication is required" } }, 401);
    });

    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText(/Username/), "portal-user");
    await user.type(screen.getByLabelText(/Password/), "correct-password");
    await user.click(screen.getByRole("button", { name: "Sign in with password" }));

    await waitFor(() =>
      expect(fetchMock.mock.calls.map(([input]) => String(input))).toEqual([
        "/me",
        "/auth/config",
        "/login",
        "/me"
      ])
    );
    expect(screen.queryByText("Dashboard page")).not.toBeInTheDocument();
    expect(showNotification).not.toHaveBeenCalledWith(
      expect.objectContaining({ title: "Signed in" })
    );
    expect(setItem).not.toHaveBeenCalledWith(SESSION_EVENT_STORAGE_KEY, "signed-in");

    await act(async () => {
      resolveConfirmation?.(response({ data: meResponse() }));
      await confirmation;
    });

    expect(await screen.findByText("Dashboard page")).toBeVisible();
    expect(showNotification).toHaveBeenCalledWith(
      expect.objectContaining({ title: "Signed in" })
    );
    expect(setItem).toHaveBeenCalledWith(SESSION_EVENT_STORAGE_KEY, "signed-in");
  });

  it("rejects a password login whose cookie session is not a password session", async () => {
    let sessionEstablished = false;
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const path = String(input);
      if (path === "/auth/config") {
        return response({ data: authConfig() });
      }
      if (path === "/login") {
        sessionEstablished = true;
        return response({ data: loginResponse() });
      }
      if (path === "/me" && sessionEstablished) {
        return response({ data: meResponse({}, "entra") });
      }
      return response({ error: { message: "Authentication is required" } }, 401);
    });

    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText(/Username/), "portal-user");
    await user.type(screen.getByLabelText(/Password/), "correct-password");
    await user.click(screen.getByRole("button", { name: "Sign in with password" }));

    expect(
      await screen.findByText("Password sign-in could not be confirmed. Try again.")
    ).toBeVisible();
    expect(screen.getByLabelText(/Username/)).toBeVisible();
    expect(screen.queryByText("Dashboard page")).not.toBeInTheDocument();
    expect(setItem).not.toHaveBeenCalledWith(SESSION_EVENT_STORAGE_KEY, "signed-in");
  });

  it("fails closed when auth config is unavailable and exposes an explicit retry", async () => {
    let configAttempts = 0;
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      if (String(input) === "/auth/config") {
        configAttempts += 1;
        if (configAttempts === 1) {
          throw new TypeError("Auth config unavailable");
        }
        return response({ data: authConfig() });
      }

      return response({ error: { message: "Authentication is required" } }, 401);
    });

    renderWithProviders(<App />);

    expect(await screen.findByText("Sign-in options unavailable")).toBeVisible();
    expect(screen.queryByLabelText(/Username/)).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Retry" }));

    expect(await screen.findByLabelText(/Username/)).toBeVisible();
    expect(configAttempts).toBe(2);
    expect(fetchMock.mock.calls.map(([input]) => String(input))).toEqual([
      "/me",
      "/auth/config",
      "/auth/config"
    ]);
  });

  it("restores an Entra callback session, returns to the route, and publishes no secret", async () => {
    window.history.replaceState(
      null,
      "",
      "/?auth_result=entra&code=must-not-persist&state=must-not-persist#catalog"
    );
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(response({ data: meResponse({}, "entra") }));

    renderWithProviders(<App />);

    expect(await screen.findByText("Catalog page")).toBeVisible();
    expect(await screen.findByText("Signed in with Microsoft")).toBeVisible();
    expect(window.location.pathname).toBe("/");
    expect(window.location.search).toBe("");
    expect(window.location.hash).toBe("#catalog");
    expect(fetchMock.mock.calls.map(([input]) => String(input))).toEqual(["/me"]);
    expect(setItem.mock.calls).toEqual([[SESSION_EVENT_STORAGE_KEY, "signed-in"]]);
    expect(window.localStorage.getItem("idp_token")).toBeNull();
    expect(window.localStorage.getItem("state")).toBeNull();
    expect(window.localStorage.getItem("code")).toBeNull();
  });

  it("cleans root OAuth secrets even when there is no recognized Entra marker", async () => {
    window.history.replaceState(
      null,
      "",
      "/?theme=compact&code=must-not-persist&state=must-not-persist&access_token=secret#catalog"
    );
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      expect(window.location.pathname).toBe("/");
      expect(window.location.search).toBe("?theme=compact");
      return response({ data: meResponse() });
    });

    renderWithProviders(<App />);

    expect(await screen.findByText("Catalog page")).toBeVisible();
    expect(window.location.search).toBe("?theme=compact");
    expect(window.location.hash).toBe("#catalog");
    expect(fetchMock.mock.calls.map(([input]) => String(input))).toEqual(["/me"]);
  });

  it("shows a whitelisted Entra failure and keeps the return route without local state", async () => {
    window.history.replaceState(
      null,
      "",
      "/?auth_error=entra_access_denied&state=must-not-persist#catalog"
    );
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) =>
      String(input) === "/auth/config"
        ? response({ data: authConfig(false, true) })
        : response({ error: { message: "Authentication is required" } }, 401)
    );

    renderWithProviders(<App />);

    expect(
      await screen.findByText("Microsoft sign-in was cancelled or denied.")
    ).toBeVisible();
    expect(screen.getByRole("link", { name: "Continue with Microsoft" })).toHaveAttribute(
      "href",
      "/auth/entra/start?return_to=%2F%23catalog"
    );
    expect(screen.queryByLabelText(/Username/)).not.toBeInTheDocument();
    expect(window.location.search).toBe("");
    expect(window.location.hash).toBe("#catalog");
    expect(fetchMock.mock.calls.map(([input]) => String(input))).toEqual([
      "/me",
      "/auth/config"
    ]);
    expect(setItem).not.toHaveBeenCalled();
    expect(window.localStorage.getItem("state")).toBeNull();
  });

  it("keeps the authenticated Graph connector OAuth callback separate from Entra", async () => {
    window.history.replaceState(
      null,
      "",
      "/oauth/microsoft/callback?code=graph-code&state=graph-state"
    );
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const path = String(input);
      if (path === "/me") {
        expect(window.location.pathname).toBe("/oauth/microsoft/callback");
        expect(window.location.search).toBe("?code=graph-code&state=graph-state");
        return response({ data: meResponse({ manage_connectors: true }) });
      }
      if (path === "/connectors/oauth/microsoft/callback") {
        expect(window.location.pathname).toBe("/");
        expect(window.location.search).toBe("");
        return response({ data: { source: "graph-mail", config: {} } });
      }
      throw new Error(`Unexpected request: ${path}`);
    });

    renderWithProviders(<App />);

    expect(await screen.findByText("Connectors page")).toBeVisible();
    expect(fetchMock.mock.calls.map(([input]) => String(input))).toEqual([
      "/me",
      "/connectors/oauth/microsoft/callback"
    ]);
    expect(JSON.parse(String(fetchMock.mock.calls[1]?.[1]?.body))).toMatchObject({
      code: "graph-code",
      state: "graph-state",
      redirect_uri: `${window.location.origin}/oauth/microsoft/callback`
    });
    expect(window.location.hash).toBe("#connectors?source=graph-mail");
    expect(setItem).not.toHaveBeenCalled();
  });

  it("syncs logout from another browser tab without a credential payload", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) =>
      String(input) === "/auth/config"
        ? response({ data: authConfig() })
        : response({ data: meResponse() })
    );

    renderWithProviders(<App />);
    expect(await screen.findByText("Dashboard page")).toBeVisible();

    fireEvent(
      window,
      new StorageEvent("storage", {
        key: SESSION_EVENT_STORAGE_KEY,
        newValue: "signed-out"
      })
    );

    expect(await screen.findByLabelText(/Username/)).toBeVisible();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("rechecks /me after another browser tab signs in", async () => {
    let sessionEstablished = false;
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      if (String(input) === "/auth/config") {
        return response({ data: authConfig() });
      }
      return sessionEstablished
        ? response({ data: meResponse() })
        : response({ error: { message: "Authentication is required" } }, 401);
    });

    renderWithProviders(<App />);
    expect(await screen.findByLabelText(/Username/)).toBeVisible();

    sessionEstablished = true;
    fireEvent(
      window,
      new StorageEvent("storage", {
        key: SESSION_EVENT_STORAGE_KEY,
        newValue: "signed-in"
      })
    );

    expect(await screen.findByText("Dashboard page")).toBeVisible();
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it("clears a prior Entra callback error after any successful /me restore", async () => {
    window.history.replaceState(
      null,
      "",
      "/?auth_error=entra_access_denied&state=must-not-persist#catalog"
    );
    let sessionEstablished = false;
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      if (String(input) === "/auth/config") {
        return response({ data: authConfig(true, true) });
      }
      return sessionEstablished
        ? response({ data: meResponse() })
        : response({ error: { message: "Authentication is required" } }, 401);
    });

    renderWithProviders(<App />);
    expect(
      await screen.findByText("Microsoft sign-in was cancelled or denied.")
    ).toBeVisible();

    sessionEstablished = true;
    fireEvent(
      window,
      new StorageEvent("storage", {
        key: SESSION_EVENT_STORAGE_KEY,
        newValue: "signed-in"
      })
    );
    expect(await screen.findByText("Catalog page")).toBeVisible();

    fireEvent(
      window,
      new StorageEvent("storage", {
        key: SESSION_EVENT_STORAGE_KEY,
        newValue: "signed-out"
      })
    );
    expect(await screen.findByLabelText(/Username/)).toBeVisible();
    expect(
      screen.queryByText("Microsoft sign-in was cancelled or denied.")
    ).not.toBeInTheDocument();
  });

  it("ignores a stale bootstrap 401 after another tab starts a fresh session probe", async () => {
    let resolveStaleProbe: ((response: Response) => void) | undefined;
    const staleProbe = new Promise<Response>((resolve) => {
      resolveStaleProbe = resolve;
    });
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementationOnce(() => staleProbe)
      .mockResolvedValueOnce(response({ data: meResponse() }));

    renderWithProviders(<App />);
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));

    fireEvent(
      window,
      new StorageEvent("storage", {
        key: SESSION_EVENT_STORAGE_KEY,
        newValue: "signed-in"
      })
    );
    expect(await screen.findByText("Dashboard page")).toBeVisible();

    await act(async () => {
      resolveStaleProbe?.(response({ error: { message: "Old session expired" } }, 401));
      await Promise.resolve();
    });

    expect(screen.getByText("Dashboard page")).toBeVisible();
    expect(screen.queryByLabelText(/Username/)).not.toBeInTheDocument();
  });
});

function meResponse(
  capabilities: Partial<MeResponse["capabilities"]> = {},
  authMethod = "password"
): MeResponse {
  return {
    id: 7,
    username: "portal-user",
    roles: ["member"],
    expires_at: "2099-01-01T00:00:00Z",
    auth_method: authMethod,
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

function loginResponse(): LoginResponse {
  return {
    expires_at: "2099-01-01T00:00:00Z",
    auth_method: "password"
  };
}

function authConfig(
  passwordLoginEnabled = true,
  entraLoginEnabled = false
): PublicAuthConfig {
  return {
    password_login_enabled: passwordLoginEnabled,
    entra_login_enabled: entraLoginEnabled
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
