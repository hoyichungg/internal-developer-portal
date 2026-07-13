import { notifications } from "@mantine/notifications";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DashboardView } from "./DashboardView";
import type { MeOverviewResponse } from "../../types/api";
import { createMockApiClient } from "../../test/mockApiClient";
import { renderWithProviders } from "../../test/render";

afterEach(() => notifications.clean());

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
    expect(screen.getByText("Platform standup")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Join" })).toHaveAttribute(
      "href",
      "https://teams.example.test/platform-standup"
    );

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

  it("marks a message read and reloads the actionable list and summary", async () => {
    const user = userEvent.setup();
    let currentOverview = overviewWithPriorityItems();
    const message = currentOverview.unread_notifications[0];
    const { client, calls } = createMockApiClient({
      "GET /me/overview": () => currentOverview,
      "POST /notifications/73/read": () => {
        currentOverview = {
          ...currentOverview,
          unread_notifications: [],
          priority_items: currentOverview.priority_items.filter(
            (item) => item.key !== "notification-73"
          ),
          summary: { ...currentOverview.summary, unread_notifications: 0 }
        };
        return { ...message, is_read: true, read_at: "2026-07-10T08:15:00Z" };
      }
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

    await user.click(await screen.findByRole("button", { name: "Mark Incident summary read" }));

    expect(await screen.findByText("No unread messages")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Mark Incident summary read" })).not.toBeInTheDocument();
    expect(screen.getAllByText("Messages")[0].parentElement).toHaveTextContent("0");
    expect(calls.filter((call) => call.path === "/me/overview")).toHaveLength(2);
    expect(calls).toContainEqual({
      method: "POST",
      path: "/notifications/73/read",
      body: undefined
    });
  });

  it("snoozes a message for one hour and sends a backend-compatible timestamp", async () => {
    const user = userEvent.setup();
    let currentOverview = overviewWithPriorityItems();
    const message = currentOverview.unread_notifications[0];
    const { client, calls } = createMockApiClient({
      "GET /me/overview": () => currentOverview,
      "POST /notifications/73/snooze": (body) => {
        currentOverview = {
          ...currentOverview,
          unread_notifications: [],
          priority_items: currentOverview.priority_items.filter(
            (item) => item.key !== "notification-73"
          ),
          summary: { ...currentOverview.summary, unread_notifications: 0 }
        };
        return {
          ...message,
          snoozed_until: (body as { snoozed_until: string }).snoozed_until
        };
      }
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

    await user.click(
      await screen.findByRole("button", { name: "Snooze Incident summary for 1 hour" })
    );

    expect(await screen.findByText("No unread messages")).toBeInTheDocument();
    const snoozeCall = calls.find((call) => call.path === "/notifications/73/snooze");
    expect((snoozeCall?.body as { snoozed_until: string }).snoozed_until).toMatch(
      /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/
    );
    expect(calls.filter((call) => call.path === "/me/overview")).toHaveLength(2);
  });

  it("keeps a message actionable when a quick action fails", async () => {
    const user = userEvent.setup();
    const { client, calls } = createMockApiClient({
      "GET /me/overview": overviewWithPriorityItems(),
      "POST /notifications/73/dismiss": () =>
        Promise.reject(new Error("Could not save notification receipt"))
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

    await user.click(await screen.findByRole("button", { name: "Dismiss Incident summary" }));

    expect((await screen.findAllByText("Could not save notification receipt")).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Mark Incident summary read" })).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Dismiss Incident summary" })).toBeEnabled()
    );
    expect(calls.filter((call) => call.path === "/me/overview")).toHaveLength(1);
  });
});

function overviewWithPriorityItems(): MeOverviewResponse {
  return {
    user: {
      id: 1,
      username: "admin",
      roles: ["admin"],
      auth_method: "password",
      capabilities: {
        manage_connectors: true,
        view_audit: true,
        manage_maintainers: true,
        view_user_directory: true
      },
      maintainer_access: []
    },
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
        last_checked_at: "2026-05-19T00:00:00Z",
        created_at: "2026-05-19T00:00:00Z",
        updated_at: "2026-05-19T00:00:00Z",
        source: "monitoring",
        external_id: "identity-api"
      }
    ],
    packages: [],
    today_calendar_events: [
      {
        id: 81,
        source: "graph-calendar",
        external_id: "evt-standup",
        title: "Platform standup",
        body: "Daily engineering sync",
        organizer: "Taylor Lin",
        location: "Teams",
        starts_at: localTodayAt(9, 30),
        ends_at: localTodayAt(10, 0),
        time_zone: "Taipei Standard Time",
        is_all_day: false,
        is_cancelled: false,
        web_url: "https://outlook.example.test/events/evt-standup",
        join_url: "https://teams.example.test/platform-standup",
        connector_id: 12,
        owner_user_id: 1,
        maintainer_id: null,
        source_updated_at: null,
        last_seen_run_id: 90,
        archived_at: null,
        created_at: localTodayAt(8, 0),
        updated_at: localTodayAt(8, 0)
      }
    ],
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
        created_at: "2026-05-19T00:00:00Z",
        updated_at: "2026-05-19T00:02:00Z"
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
        source_is_read: false,
        url: null,
        created_at: "2026-05-19T00:00:00Z",
        updated_at: "2026-05-19T00:04:00Z",
        connector_id: 12,
        owner_user_id: 1,
        maintainer_id: null,
        source_updated_at: "2026-05-19T00:04:00Z",
        last_seen_run_id: 91,
        archived_at: null,
        read_at: null,
        dismissed_at: null,
        snoozed_until: null
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
        started_at: "2026-05-19T00:00:00Z",
        finished_at: "2026-05-19T00:01:00Z",
        trigger: "scheduled",
        claimed_at: null,
        worker_id: "worker-1",
        attempt_count: 1,
        max_attempts: 3,
        next_attempt_at: "2026-05-19T00:00:00Z",
        lease_expires_at: null,
        heartbeat_at: "2026-05-19T00:00:15Z",
        cancel_requested_at: null,
        cancelled_at: null,
        parent_run_id: null,
        snapshot_complete: null,
        archived_count: 0
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
        occurred_at: "2026-05-19T00:00:00Z"
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
        occurred_at: "2026-05-19T00:01:00Z"
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
        occurred_at: "2026-05-19T00:02:00Z"
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
        occurred_at: "2026-05-19T00:04:00Z"
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
      latest_worker_seen_at: "2026-05-19T00:00:00Z",
      worker_stale_after_seconds: 45,
      latest_retention_cleanup: null,
      latest_health_check_at: "2026-05-19T00:00:00Z",
      health_data_stale: false,
      health_stale_after_hours: 24
    },
    summary: {
      maintainers: 0,
      services: 1,
      unhealthy_services: 1,
      packages: 0,
      today_calendar_events: 1,
      open_work_cards: 1,
      unread_notifications: 1,
      failed_connector_runs: 1
    }
  };
}

function localTodayAt(hours: number, minutes: number): string {
  const date = new Date();
  date.setHours(hours, minutes, 0, 0);
  return date.toISOString();
}
