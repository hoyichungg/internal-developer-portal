import { describe, expect, it, vi } from "vitest";

import type { ApiClient } from "../../api/client";
import { performNotificationAction, snoozeUntilForPreset } from "./notificationActions";

describe("notification datetime actions", () => {
  it("sends one-hour snooze as an unambiguous UTC RFC3339 instant", () => {
    expect(snoozeUntilForPreset("one-hour", new Date("2026-07-10T09:00:00+08:00"))).toBe(
      "2026-07-10T02:00:00.000Z"
    );
  });

  it("preserves an explicit non-UTC offset when a caller supplies one", async () => {
    const post = vi.fn().mockResolvedValue({ id: 73 });
    const client = { post } as unknown as ApiClient;

    await performNotificationAction(client, 73, "snooze", {
      snoozedUntil: "2026-07-10T09:00:00+08:00"
    });

    expect(post).toHaveBeenCalledWith("/notifications/73/snooze", {
      snoozed_until: "2026-07-10T09:00:00+08:00"
    });
  });

  it("refuses an ambiguous snooze timestamp before making a request", async () => {
    const post = vi.fn();
    const client = { post } as unknown as ApiClient;

    await expect(
      performNotificationAction(client, 73, "snooze", {
        snoozedUntil: "2026-07-10T09:00:00"
      })
    ).rejects.toThrow("RFC3339");
    expect(post).not.toHaveBeenCalled();
  });
});
