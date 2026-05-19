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

describe("ConnectorsView config editor", () => {
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
      expect(configInput.value).not.toContain("real-graph-token");
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
    expect(submittedConfig.access_token).toBe("***redacted***");
    expect(submittedConfig.top).toBe(10);
    expect(payload.config).not.toContain("real-graph-token");
  });
});

describe("ConnectorsView run detail drilldown", () => {
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
    last_run_at: "2026-05-19T00:00:00",
    last_success_at: "2026-05-19T00:00:00",
    created_at: "2026-05-19T00:00:00",
    updated_at: "2026-05-19T00:00:00"
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
        access_token: "***redacted***",
        time_zone: "Taipei Standard Time",
        lookahead_hours: 24,
        top: 25
      },
      null,
      2
    ),
    sample_payload: JSON.stringify({ items: [] }, null, 2),
    created_at: "2026-05-19T00:00:00",
    updated_at: "2026-05-19T00:00:00",
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
    started_at: "2026-05-19T00:00:00",
    finished_at: "2026-05-19T00:00:01",
    trigger: "scheduled",
    claimed_at: "2026-05-19T00:00:00",
    worker_id: "worker-1"
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
        created_at: "2026-05-19T00:00:01"
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
        created_at: "2026-05-19T00:00:01"
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
