import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { DashboardView } from "./DashboardView";
import type { MeOverviewResponse } from "../../types/api";
import { createMockApiClient } from "../../test/mockApiClient";
import { renderWithProviders } from "../../test/render";

describe("DashboardView daily workbench", () => {
  it("routes each today-first item to the expected detail surface", async () => {
    const user = userEvent.setup();
    const onOpenService = vi.fn();
    const onOpenConnector = vi.fn();
    const onOpenWorkCard = vi.fn();
    const onOpenNotification = vi.fn();
    const { client } = createMockApiClient({
      "GET /me/overview": overviewWithPriorityItems()
    });

    renderWithProviders(
      <DashboardView
        client={client}
        onOpenService={onOpenService}
        onOpenConnector={onOpenConnector}
        onOpenWorkCard={onOpenWorkCard}
        onOpenNotification={onOpenNotification}
      />
    );

    expect(
      await screen.findByRole("button", { name: "Overview Identity API" })
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Run detail graph-calendar / notifications" })
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Details Fix blocked deployment" })
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Details Incident summary" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Overview Identity API" }));
    expect(onOpenService).toHaveBeenCalledWith(7);

    await user.click(
      screen.getByRole("button", { name: "Run detail graph-calendar / notifications" })
    );
    expect(onOpenConnector).toHaveBeenCalledWith({
      source: "graph-calendar",
      target: "notifications",
      runId: 91
    });

    await user.click(screen.getByRole("button", { name: "Details Fix blocked deployment" }));
    expect(onOpenWorkCard).toHaveBeenCalledWith(42);

    await user.click(screen.getByRole("button", { name: "Details Incident summary" }));
    expect(onOpenNotification).toHaveBeenCalledWith(73);
  });

  it("filters, searches, and sorts the daily workbench", async () => {
    const user = userEvent.setup();
    const { client } = createMockApiClient({
      "GET /me/overview": overviewWithPriorityItems()
    });

    renderWithProviders(
      <DashboardView
        client={client}
        onOpenService={vi.fn()}
        onOpenConnector={vi.fn()}
        onOpenWorkCard={vi.fn()}
        onOpenNotification={vi.fn()}
      />
    );

    const workbench = await screen.findByLabelText("Daily workbench");
    expect(within(workbench).getByText("Identity API")).toBeInTheDocument();
    expect(within(workbench).getByText("Fix blocked deployment")).toBeInTheDocument();

    await user.click(within(workbench).getByRole("radio", { name: "Work" }));
    expect(within(workbench).getByText("Fix blocked deployment")).toBeInTheDocument();
    expect(within(workbench).queryByText("Identity API")).not.toBeInTheDocument();

    await user.click(within(workbench).getByRole("radio", { name: "All" }));
    await user.type(within(workbench).getByLabelText("Search workbench"), "incident");
    expect(within(workbench).getByText("Incident summary")).toBeInTheDocument();
    expect(within(workbench).queryByText("Fix blocked deployment")).not.toBeInTheDocument();

    await user.click(within(workbench).getByRole("button", { name: "Clear" }));
    await user.click(within(workbench).getByLabelText("Sort workbench"));
    await user.click(await screen.findByRole("option", { name: "Newest first" }));

    const rows = within(workbench).getAllByTestId("workbench-row");
    expect(within(rows[0]).getByText("Incident summary")).toBeInTheDocument();
  });
});

function overviewWithPriorityItems(): MeOverviewResponse {
  return {
    user: { id: 1, username: "admin", roles: ["admin"] },
    maintainers: [],
    services: [
      {
        id: 7,
        maintainer_id: 1,
        slug: "identity-api",
        name: "Identity API",
        lifecycle_status: "active",
        health_status: "down",
        description: "Authentication service",
        repository_url: null,
        dashboard_url: null,
        runbook_url: null,
        last_checked_at: "2026-05-19T00:00:00",
        created_at: "2026-05-19T00:00:00",
        updated_at: "2026-05-19T00:00:00",
        source: "monitoring",
        external_id: "identity-api"
      }
    ],
    packages: [],
    open_work_cards: [
      {
        id: 42,
        source: "azure-devops",
        external_id: "ADO-42",
        title: "Fix blocked deployment",
        status: "blocked",
        priority: "urgent",
        assignee: "platform-team",
        due_at: null,
        url: null,
        created_at: "2026-05-19T00:00:00",
        updated_at: "2026-05-19T00:02:00"
      }
    ],
    unread_notifications: [
      {
        id: 73,
        source: "graph-calendar",
        external_id: "evt-incident",
        title: "Incident summary",
        body: "Production incident review starts at 14:00.",
        severity: "critical",
        is_read: false,
        url: null,
        created_at: "2026-05-19T00:00:00",
        updated_at: "2026-05-19T00:04:00"
      }
    ],
    failed_connector_runs: [
      {
        id: 91,
        source: "graph-calendar",
        target: "notifications",
        status: "failed",
        success_count: 0,
        failure_count: 2,
        duration_ms: 120,
        error_message: "Graph request returned 401",
        started_at: "2026-05-19T00:00:00",
        finished_at: "2026-05-19T00:01:00",
        trigger: "scheduled",
        claimed_at: null,
        worker_id: "worker-1"
      }
    ],
    priority_items: [
      {
        key: "service-7",
        kind: "service",
        severity: "down",
        title: "Identity API",
        detail: "down - monitoring",
        source: "monitoring",
        target: "service_health",
        service_id: 7,
        occurred_at: "2026-05-19T00:00:00"
      },
      {
        key: "run-91",
        kind: "connector_run",
        severity: "failed",
        title: "graph-calendar / notifications",
        detail: "Graph request returned 401",
        source: "graph-calendar",
        target: "notifications",
        record_id: 91,
        occurred_at: "2026-05-19T00:01:00"
      },
      {
        key: "work-42",
        kind: "work_card",
        severity: "blocked",
        title: "Fix blocked deployment",
        detail: "urgent - platform-team - azure-devops",
        source: "azure-devops",
        target: "work_cards",
        record_id: 42,
        occurred_at: "2026-05-19T00:02:00"
      },
      {
        key: "notification-73",
        kind: "notification",
        severity: "critical",
        title: "Incident summary",
        detail: "graph-calendar",
        source: "graph-calendar",
        target: "notifications",
        record_id: 73,
        occurred_at: "2026-05-19T00:04:00"
      }
    ],
    health_history: {
      summary: {
        window_hours: 24,
        checks: 0,
        healthy_checks: 0,
        degraded_checks: 0,
        down_checks: 0,
        unknown_checks: 0,
        changed_checks: 0
      },
      recent_checks: [],
      recent_incidents: []
    },
    operations: {
      worker_status: "healthy",
      active_workers: 1,
      stale_workers: 0,
      latest_worker_seen_at: "2026-05-19T00:00:00",
      worker_stale_after_seconds: 45,
      latest_retention_cleanup: null,
      latest_health_check_at: "2026-05-19T00:00:00",
      health_data_stale: false,
      health_stale_after_hours: 24
    },
    summary: {
      maintainers: 0,
      services: 1,
      unhealthy_services: 1,
      packages: 0,
      open_work_cards: 1,
      unread_notifications: 1,
      failed_connector_runs: 1
    }
  };
}
