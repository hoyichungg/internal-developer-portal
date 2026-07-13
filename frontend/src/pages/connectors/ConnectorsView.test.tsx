import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ConnectorsView } from "./ConnectorsView";
import type {
  Connector,
  ConnectorConfigResponse,
  ConnectorOperationsResponse,
  ConnectorRun,
  ConnectorRunDetail
} from "../../types/api";
import { createMockApiClient } from "../../test/mockApiClient";
import { renderWithProviders } from "../../test/render";
import { ApiError } from "../../api/client";

describe("ConnectorsView connector visibility", () => {
  it("submits the selected maintainer scope instead of silently creating a global connector", async () => {
    const user = userEvent.setup();
    const postBodies: unknown[] = [];
    const scopedConnector: Connector = {
      ...graphCalendarConnector(),
      source: "team-devops",
      display_name: "Team DevOps",
      scope_type: "maintainer",
      maintainer_id: 7
    };
    const { client } = createMockApiClient({
      "GET /connectors": [],
      "GET /connectors/operations": emptyOperations(),
      "GET /maintainers": [
        {
          id: 7,
          display_name: "Platform Team",
          email: "platform@example.test",
          created_at: "2026-07-10T08:00:00Z"
        }
      ],
      "GET /users": [
        { id: 11, username: "alice", roles: ["member"], created_at: "2026-07-10T08:00:00Z" }
      ],
      "POST /connectors": (body) => {
        postBodies.push(body);
        return scopedConnector;
      }
    });

    renderWithProviders(
      <ConnectorsView client={client} drillTarget={null} onOpenService={vi.fn()} />
    );

    await user.click(await screen.findByRole("button", { name: "Create connector" }));
    await user.type(screen.getByPlaceholderText("azure-devops"), "team-devops");
    await user.type(screen.getByPlaceholderText("azure_devops"), "azure_devops");
    await user.type(screen.getByPlaceholderText("Azure DevOps"), "Team DevOps");
    await user.click(screen.getByPlaceholderText("Choose a visibility scope"));
    await user.click(screen.getByRole("option", { name: "One maintainer team" }));
    await user.click(await screen.findByPlaceholderText("Choose a team"));
    await user.click(screen.getByRole("option", { name: "Platform Team" }));
    await user.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => expect(postBodies).toHaveLength(1));
    expect(postBodies[0]).toEqual({
      source: "team-devops",
      kind: "azure_devops",
      display_name: "Team DevOps",
      status: "active",
      scope_type: "maintainer",
      owner_user_id: null,
      maintainer_id: 7
    });
  });

  it("moves an existing connector to a selected team scope", async () => {
    const user = userEvent.setup();
    const putBodies: unknown[] = [];
    const scopedConnector: Connector = {
      ...graphCalendarConnector(),
      scope_type: "maintainer",
      maintainer_id: 7
    };
    const { client } = createMockApiClient({
      "GET /connectors": [graphCalendarConnector()],
      "GET /connectors/operations": emptyOperations(),
      "GET /connectors/graph-calendar/config": graphCalendarConfigResponse(),
      "GET /connectors/runs?source=graph-calendar": [],
      "GET /maintainers": [
        {
          id: 7,
          display_name: "Platform Team",
          email: "platform@example.test",
          created_at: "2026-07-10T08:00:00Z"
        }
      ],
      "GET /users": [],
      "PUT /connectors/graph-calendar/scope": (body) => {
        putBodies.push(body);
        return scopedConnector;
      }
    });

    renderWithProviders(
      <ConnectorsView client={client} drillTarget={null} onOpenService={vi.fn()} />
    );

    await waitFor(() => expect(screen.getByLabelText("Config JSON")).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "Edit visibility" }));
    await user.click(screen.getByDisplayValue("Everyone"));
    await user.click(screen.getByRole("option", { name: "One maintainer team" }));
    await user.click(await screen.findByPlaceholderText("Choose a team"));
    await user.click(
      screen.getByRole("option", { name: "Platform Team (platform@example.test)" })
    );
    await user.click(screen.getByRole("button", { name: "Save visibility" }));

    await waitFor(() => expect(putBodies).toHaveLength(1));
    expect(putBodies[0]).toEqual({
      scope_type: "maintainer",
      owner_user_id: null,
      maintainer_id: 7
    });
    expect(await screen.findByText(/team #7/)).toBeInTheDocument();
  });
});

describe("ConnectorsView config editor", () => {
  it("uses an editable default only when the config endpoint returns 404", async () => {
    const user = userEvent.setup();
    const putBodies: unknown[] = [];
    const { client } = createMockApiClient({
      "GET /connectors": [graphCalendarConnector()],
      "GET /connectors/operations": emptyOperations(),
      "GET /connectors/graph-calendar/config": () => {
        throw new ApiError("Connector config was not found", { kind: "http", status: 404 });
      },
      "GET /connectors/runs?source=graph-calendar": [],
      "PUT /connectors/graph-calendar/config": (body) => {
        putBodies.push(body);
        return graphCalendarConfigResponse(body as Partial<ConnectorConfigResponse>);
      }
    });

    renderWithProviders(
      <ConnectorsView client={client} drillTarget={null} onOpenService={vi.fn()} />
    );

    const configInput = (await screen.findByLabelText("Config JSON")) as HTMLTextAreaElement;
    await waitFor(() => expect(configInput).toBeEnabled());
    expect(configInput.value).toContain('"adapter": "azure_devops"');
    expect(screen.queryByText("Connector config unavailable")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save config" }));
    await waitFor(() => expect(putBodies).toHaveLength(1));
  });

  it.each([
    ["forbidden", new ApiError("Admin role is required", { kind: "http", status: 403 })],
    ["server failure", new ApiError("Database unavailable", { kind: "http", status: 503 })],
    ["network failure", new ApiError("Unable to reach the API", { kind: "network" })]
  ])("locks config writes and offers retry after a %s", async (_label, loadError) => {
    const user = userEvent.setup();
    const put = vi.fn();
    const { client } = createMockApiClient({
      "GET /connectors": [graphCalendarConnector()],
      "GET /connectors/operations": emptyOperations(),
      "GET /connectors/graph-calendar/config": () => {
        throw loadError;
      },
      "GET /connectors/runs?source=graph-calendar": [],
      "PUT /connectors/graph-calendar/config": put
    });

    renderWithProviders(
      <ConnectorsView client={client} drillTarget={null} onOpenService={vi.fn()} />
    );

    expect(await screen.findByText("Connector config unavailable")).toBeInTheDocument();
    expect(screen.getAllByText(loadError.message).length).toBeGreaterThan(0);
    expect(screen.getByLabelText("Config JSON")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Save config" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Retry config" })).toBeEnabled();

    await user.click(screen.getByRole("button", { name: "Save config" }));
    expect(put).not.toHaveBeenCalled();
  });

  it("unlocks the preserved config editor after a failed load is retried successfully", async () => {
    const user = userEvent.setup();
    let attempts = 0;
    const { client } = createMockApiClient({
      "GET /connectors": [graphCalendarConnector()],
      "GET /connectors/operations": emptyOperations(),
      "GET /connectors/graph-calendar/config": () => {
        attempts += 1;
        if (attempts === 1) {
          throw new ApiError("Database unavailable", { kind: "http", status: 503 });
        }
        return graphCalendarConfigResponse();
      },
      "GET /connectors/runs?source=graph-calendar": []
    });

    renderWithProviders(
      <ConnectorsView client={client} drillTarget={null} onOpenService={vi.fn()} />
    );

    await user.click(await screen.findByRole("button", { name: "Retry config" }));

    const configInput = screen.getByLabelText("Config JSON") as HTMLTextAreaElement;
    await waitFor(() => expect(configInput).toBeEnabled());
    expect(configInput.value).toContain("***redacted***");
    expect(screen.queryByText("Connector config unavailable")).not.toBeInTheDocument();
    expect(attempts).toBe(2);
  });

  it("submits redacted connector config back unchanged during a safe round-trip", async () => {
    const user = userEvent.setup();
    const putBodies: unknown[] = [];
    const { client } = createMockApiClient({
      "GET /connectors": [graphCalendarConnector()],
      "GET /connectors/operations": emptyOperations(),
      "GET /connectors/graph-calendar/config": graphCalendarConfigResponse(),
      "GET /connectors/runs?source=graph-calendar": [],
      "PUT /connectors/graph-calendar/config": (body) => {
        putBodies.push(body);
        return graphCalendarConfigResponse(body as Partial<ConnectorConfigResponse>);
      }
    });

    renderWithProviders(
      <ConnectorsView client={client} drillTarget={null} onOpenService={vi.fn()} />
    );

    const configInput = (await screen.findByLabelText("Config JSON")) as HTMLTextAreaElement;
    await waitFor(() => {
      expect(configInput.value).toContain("***redacted***");
      expect(configInput.value).not.toContain("real-graph-refresh-token");
    });

    const updatedConfig = {
      ...JSON.parse(configInput.value),
      top: 10
    };
    fireEvent.change(configInput, {
      target: { value: JSON.stringify(updatedConfig, null, 2) }
    });

    await user.click(screen.getByRole("button", { name: "Save config" }));

    await waitFor(() => expect(putBodies).toHaveLength(1));
    const payload = putBodies[0] as { config: string; target: string; schedule_cron: string };
    const submittedConfig = JSON.parse(payload.config);

    expect(payload.target).toBe("notifications");
    expect(payload.schedule_cron).toBe("@every 15m");
    expect(submittedConfig.client_secret).toBe("***redacted***");
    expect(submittedConfig.refresh_token).toBe("***redacted***");
    expect(submittedConfig.top).toBe(10);
    expect(payload.config).not.toContain("real-graph-refresh-token");
  });

  it("starts Microsoft OAuth after saving the current connector config", async () => {
    const user = userEvent.setup();
    const openSpy = vi.spyOn(window, "open").mockImplementation(() => null);
    const putBodies: unknown[] = [];
    const authorizeBodies: unknown[] = [];
    const { client, calls } = createMockApiClient({
      "GET /connectors": [graphCalendarConnector()],
      "GET /connectors/operations": emptyOperations(),
      "GET /connectors/graph-calendar/config": graphCalendarConfigResponse(),
      "GET /connectors/runs?source=graph-calendar": [],
      "PUT /connectors/graph-calendar/config": (body) => {
        putBodies.push(body);
        return graphCalendarConfigResponse(body as Partial<ConnectorConfigResponse>);
      },
      "POST /connectors/graph-calendar/oauth/microsoft/authorize": (body) => {
        authorizeBodies.push(body);
        return {
          authorization_url: "https://login.microsoftonline.test/authorize?state=abc",
          state: "abc",
          redirect_uri: (body as { redirect_uri: string }).redirect_uri,
          scope: "https://graph.microsoft.com/Calendars.Read offline_access",
          expires_at: "2026-05-19T00:10:00Z"
        };
      }
    });

    try {
      renderWithProviders(
        <ConnectorsView client={client} drillTarget={null} onOpenService={vi.fn()} />
      );

      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Reconnect Microsoft" })).toBeEnabled()
      );
      await user.click(screen.getByRole("button", { name: "Reconnect Microsoft" }));

      await waitFor(() => expect(putBodies).toHaveLength(1));
      await waitFor(() => expect(authorizeBodies).toHaveLength(1));
      await waitFor(() => expect(openSpy).toHaveBeenCalledTimes(1));
      expect(openSpy).toHaveBeenCalledWith(
        "https://login.microsoftonline.test/authorize?state=abc",
        "_self",
        "noopener"
      );
      expect(putBodies).toHaveLength(1);
      expect(authorizeBodies).toEqual([
        { redirect_uri: `${window.location.origin}/oauth/microsoft/callback` }
      ]);
      expect(calls.map((call) => `${call.method} ${call.path}`)).toEqual(
        expect.arrayContaining([
          "PUT /connectors/graph-calendar/config",
          "POST /connectors/graph-calendar/oauth/microsoft/authorize"
        ])
      );
    } finally {
      openSpy.mockRestore();
    }
  });
});

describe("ConnectorsView run detail drilldown", () => {
  it("lets an operator cancel queued work and reloads the resulting state", async () => {
    const user = userEvent.setup();
    const queuedRun: ConnectorRun = {
      ...graphCalendarRun(),
      status: "queued",
      success_count: 0,
      failure_count: 0,
      error_message: null,
      finished_at: null,
      claimed_at: null,
      worker_id: null,
      attempt_count: 0,
      heartbeat_at: null
    };
    const cancelledRun: ConnectorRun = {
      ...queuedRun,
      status: "cancelled",
      cancel_requested_at: "2026-05-19T00:00:10Z",
      cancelled_at: "2026-05-19T00:00:10Z",
      finished_at: "2026-05-19T00:00:10Z"
    };
    const { client, calls } = createMockApiClient({
      "GET /connectors": [graphCalendarConnector()],
      "GET /connectors/operations": emptyOperations(),
      "GET /connectors/graph-calendar/config": graphCalendarConfigResponse(),
      "GET /connectors/runs?source=graph-calendar": [queuedRun],
      "POST /connectors/runs/77/cancel": cancelledRun,
      "GET /connectors/runs/77": {
        ...graphCalendarRunDetail(),
        run: cancelledRun
      }
    });

    renderWithProviders(
      <ConnectorsView client={client} drillTarget={null} onOpenService={vi.fn()} />
    );

    await waitFor(() => expect(screen.getByLabelText("Config JSON")).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "Cancel run #77" }));

    await waitFor(() =>
      expect(calls).toContainEqual({
        method: "POST",
        path: "/connectors/runs/77/cancel",
        body: {}
      })
    );
    expect(await screen.findByText("Cancelled")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeEnabled();
  });

  it("loads the requested run detail from an incoming drill target", async () => {
    const { client, calls } = createMockApiClient({
      "GET /connectors": [graphCalendarConnector()],
      "GET /connectors/operations": emptyOperations(),
      "GET /connectors/graph-calendar/config": graphCalendarConfigResponse(),
      "GET /connectors/runs?source=graph-calendar": [graphCalendarRun()],
      "GET /connectors/runs/77": graphCalendarRunDetail()
    });

    renderWithProviders(
      <ConnectorsView
        client={client}
        drillTarget={{ source: "graph-calendar", target: "notifications", runId: 77 }}
        onOpenService={vi.fn()}
      />
    );

    expect(await screen.findByText("Run #77")).toBeInTheDocument();
    expect(screen.getByText("graph-calendar - notifications - scheduled")).toBeInTheDocument();
    expect(screen.getByText("Complete - item errors, no archive")).toBeInTheDocument();
    expect(screen.getByText("evt-standup")).toBeInTheDocument();
    expect(screen.getByText("Bad event payload")).toBeInTheDocument();
    expect(calls).toEqual(
      expect.arrayContaining([{ method: "GET", path: "/connectors/runs/77", body: undefined }])
    );
  });
});

function graphCalendarConnector(): Connector {
  return {
    id: 1,
    source: "graph-calendar",
    kind: "microsoft_graph_calendar",
    display_name: "Microsoft Graph Calendar",
    status: "active",
    last_run_at: "2026-05-19T00:00:00Z",
    last_success_at: "2026-05-19T00:00:00Z",
    scope_type: "global",
    owner_user_id: null,
    maintainer_id: null,
    created_at: "2026-05-19T00:00:00Z",
    updated_at: "2026-05-19T00:00:00Z"
  };
}

function graphCalendarConfigResponse(
  overrides: Partial<ConnectorConfigResponse> = {}
): ConnectorConfigResponse {
  return {
    id: 1,
    source: "graph-calendar",
    target: "notifications",
    enabled: true,
    schedule_cron: "@every 15m",
    config: JSON.stringify(
      {
        adapter: "microsoft_graph_calendar",
        user_id: "me",
        tenant_id: "organizations",
        client_id: "graph-client-id",
        client_secret: "***redacted***",
        refresh_token: "***redacted***",
        scope: "https://graph.microsoft.com/Calendars.Read offline_access",
        time_zone: "Taipei Standard Time",
        lookahead_hours: 24,
        top: 25
      },
      null,
      2
    ),
    sample_payload: JSON.stringify({ items: [] }, null, 2),
    created_at: "2026-05-19T00:00:00Z",
    updated_at: "2026-05-19T00:00:00Z",
    last_scheduled_at: null,
    next_run_at: null,
    last_scheduled_run_id: null,
    ...overrides
  };
}

function graphCalendarRun(): ConnectorRun {
  return {
    id: 77,
    source: "graph-calendar",
    target: "notifications",
    status: "partial_success",
    success_count: 1,
    failure_count: 1,
    duration_ms: 340,
    error_message: "1 item failed",
    started_at: "2026-05-19T00:00:00Z",
    finished_at: "2026-05-19T00:00:01Z",
    trigger: "scheduled",
    claimed_at: "2026-05-19T00:00:00Z",
    worker_id: "worker-1",
    attempt_count: 1,
    max_attempts: 3,
    next_attempt_at: "2026-05-19T00:00:00Z",
    lease_expires_at: null,
    heartbeat_at: "2026-05-19T00:00:15Z",
    cancel_requested_at: null,
    cancelled_at: null,
    parent_run_id: null,
    snapshot_complete: true,
    archived_count: 0
  };
}

function graphCalendarRunDetail(): ConnectorRunDetail {
  return {
    run: graphCalendarRun(),
    items: [
      {
        id: 1,
        connector_run_id: 77,
        source: "graph-calendar",
        target: "notifications",
        record_id: 73,
        external_id: "evt-standup",
        status: "imported",
        snapshot: JSON.stringify({ title: "Calendar: Platform standup" }),
        created_at: "2026-05-19T00:00:01Z"
      }
    ],
    item_errors: [
      {
        id: 2,
        connector_run_id: 77,
        source: "graph-calendar",
        target: "notifications",
        external_id: "evt-bad",
        message: "Bad event payload",
        raw_item: JSON.stringify({ id: null }),
        created_at: "2026-05-19T00:00:01Z"
      }
    ],
    health_checks: []
  };
}

function emptyOperations(): ConnectorOperationsResponse {
  return {
    stale_after_seconds: 45,
    workers: [],
    maintenance_runs: []
  };
}
