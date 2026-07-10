import type { ApiClient } from "../../api/client";
import type { ApiId, Notification } from "../../types/api";

export type NotificationAction = "read" | "unread" | "dismiss" | "restore" | "snooze";
export type SnoozePreset = "one-hour" | "tomorrow";

export async function performNotificationAction(
  client: ApiClient,
  notificationId: ApiId,
  action: NotificationAction,
  options: { snoozedUntil?: string } = {}
): Promise<Notification> {
  const path = `/notifications/${encodeURIComponent(String(notificationId))}/${action}`;

  if (action === "snooze") {
    if (!options.snoozedUntil) {
      throw new Error("Choose when the notification should return.");
    }

    return client.post<Notification>(path, { snoozed_until: options.snoozedUntil });
  }

  return client.post<Notification>(path);
}

export function snoozeUntilForPreset(preset: SnoozePreset, now = new Date()): string {
  if (preset === "one-hour") {
    return toNaiveUtc(new Date(now.getTime() + 60 * 60 * 1000));
  }

  const tomorrowMorning = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + 1,
    9,
    0,
    0,
    0
  );
  return toNaiveUtc(tomorrowMorning);
}

function toNaiveUtc(value: Date): string {
  // The backend currently accepts a UTC NaiveDateTime, so omit the timezone suffix and milliseconds.
  return value.toISOString().slice(0, 19);
}
