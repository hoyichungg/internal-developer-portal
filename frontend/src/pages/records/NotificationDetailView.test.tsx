import { notifications } from "@mantine/notifications";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { createMockApiClient } from "../../test/mockApiClient";
import { renderWithProviders } from "../../test/render";
import type { Notification } from "../../types/api";
import { NotificationDetailView } from "./NotificationDetailView";

afterEach(() => notifications.clean());

describe("NotificationDetailView actions", () => {
  it("marks a notification read and unread using the receipt endpoints", async () => {
    const user = userEvent.setup();
    const unread = notification();
    const read = {
      ...unread,
      is_read: true,
      read_at: "2026-07-10T08:15:00"
    };
    const { client, calls } = createMockApiClient({
      "GET /notifications/73": unread,
      "POST /notifications/73/read": read,
      "POST /notifications/73/unread": unread
    });

    renderDetail(client);

    await user.click(await screen.findByRole("button", { name: "Mark read" }));
    expect(await screen.findByRole("button", { name: "Mark unread" })).toBeInTheDocument();
    expect(calls).toContainEqual({ method: "POST", path: "/notifications/73/read", body: undefined });

    await user.click(screen.getByRole("button", { name: "Mark unread" }));
    expect(await screen.findByRole("button", { name: "Mark read" })).toBeInTheDocument();
    expect(calls).toContainEqual({
      method: "POST",
      path: "/notifications/73/unread",
      body: undefined
    });
  });

  it("restores a dismissed notification loaded directly from its detail URL", async () => {
    const user = userEvent.setup();
    const restored = notification();
    const dismissed = notification({ dismissed_at: "2026-07-10T08:10:00" });
    const { client, calls } = createMockApiClient({
      "GET /notifications/73": dismissed,
      "POST /notifications/73/restore": restored
    });

    renderDetail(client);

    await user.click(await screen.findByRole("button", { name: "Restore" }));
    expect(await screen.findByRole("button", { name: "Dismiss" })).toBeInTheDocument();
    expect(calls).toContainEqual({
      method: "POST",
      path: "/notifications/73/restore",
      body: undefined
    });
  });

  it("snoozes a notification until tomorrow", async () => {
    const user = userEvent.setup();
    const initial = notification();
    const { client, calls } = createMockApiClient({
      "GET /notifications/73": initial,
      "POST /notifications/73/snooze": (body) => ({
        ...initial,
        snoozed_until: (body as { snoozed_until: string }).snoozed_until
      })
    });

    renderDetail(client);

    await user.click(await screen.findByRole("textbox", { name: "Snooze duration" }));
    await user.click(await screen.findByRole("option", { name: "Until tomorrow" }));
    await user.click(screen.getByRole("button", { name: "Snooze" }));

    expect(await screen.findByRole("button", { name: "Restore" })).toBeInTheDocument();
    const snoozeCall = calls.find((call) => call.path === "/notifications/73/snooze");
    expect(snoozeCall).toMatchObject({ method: "POST" });
    expect((snoozeCall?.body as { snoozed_until: string }).snoozed_until).toMatch(
      /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}$/
    );
  });

  it("keeps the current notification visible when an action fails", async () => {
    const user = userEvent.setup();
    const initial = notification();
    const { client } = createMockApiClient({
      "GET /notifications/73": initial,
      "POST /notifications/73/dismiss": () => Promise.reject(new Error("Receipt service unavailable"))
    });

    renderDetail(client);

    await user.click(await screen.findByRole("button", { name: "Dismiss" }));

    expect((await screen.findAllByText("Receipt service unavailable")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("Incident summary").length).toBeGreaterThan(0);
    await waitFor(() => expect(screen.getByRole("button", { name: "Dismiss" })).toBeEnabled());
    expect(screen.queryByRole("button", { name: "Restore" })).not.toBeInTheDocument();
  });
});

function renderDetail(client: ReturnType<typeof createMockApiClient>["client"]) {
  return renderWithProviders(
    <NotificationDetailView
      client={client}
      notificationId={73}
      onBack={vi.fn()}
      onOpenConnector={vi.fn()}
    />
  );
}

function notification(overrides: Partial<Notification> = {}): Notification {
  return {
    id: 73,
    source: "graph-mail",
    external_id: "message-73",
    title: "Incident summary",
    body: "Production incident review starts at 14:00.",
    severity: "critical",
    is_read: false,
    source_is_read: false,
    url: null,
    created_at: "2026-07-10T08:00:00",
    updated_at: "2026-07-10T08:05:00",
    connector_id: 12,
    owner_user_id: 1,
    maintainer_id: null,
    source_updated_at: "2026-07-10T08:05:00",
    last_seen_run_id: 91,
    archived_at: null,
    read_at: null,
    dismissed_at: null,
    snoozed_until: null,
    ...overrides
  };
}
