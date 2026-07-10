import { Alert, Box, Button, Grid, Group, Paper, Select, Stack, Text } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { IconArrowLeft, IconArrowRight, IconExternalLink } from "@tabler/icons-react";
import { useEffect, useState } from "react";
import type { ReactNode } from "react";

import type { ApiClient } from "../../api/client";
import { DataPanel } from "../../components/DataPanel";
import { EmptyText } from "../../components/EmptyText";
import { PageLoader } from "../../components/LoadingState";
import { DateCell, StatusBadge } from "../../components/tableCells";
import { ViewFrame } from "../../components/ViewFrame";
import { useAsyncData } from "../../hooks/useAsyncData";
import { useRefresh } from "../../hooks/useRefresh";
import type { ApiId, ConnectorDrillTarget, Notification } from "../../types/api";
import { showError } from "../../utils/notifications";
import {
  performNotificationAction,
  snoozeUntilForPreset,
  type NotificationAction,
  type SnoozePreset
} from "./notificationActions";

export function NotificationDetailView({
  client,
  notificationId,
  onBack,
  onOpenConnector
}: {
  client: ApiClient;
  notificationId: ApiId;
  onBack: () => void;
  onOpenConnector: (target: ConnectorDrillTarget) => void;
}) {
  const [data, actions] = useAsyncData<Notification>(
    () => client.get<Notification>(`/notifications/${encodeURIComponent(String(notificationId))}`),
    [client, notificationId]
  );
  const [actionResult, setActionResult] = useState<{
    notificationId: ApiId;
    value: Notification;
  } | null>(null);
  const [pendingAction, setPendingAction] = useState<NotificationAction | null>(null);
  const [actionError, setActionError] = useState<Error | null>(null);
  const [snoozePreset, setSnoozePreset] = useState<SnoozePreset>("one-hour");

  useRefresh(actions.reload);

  useEffect(() => {
    setActionResult(null);
    setActionError(null);
  }, [data.value, notificationId]);

  const loadedNotification =
    data.value && String(data.value.id) === String(notificationId) ? data.value : null;
  const notification =
    actionResult && String(actionResult.notificationId) === String(notificationId)
      ? actionResult.value
      : loadedNotification;

  async function runAction(action: NotificationAction) {
    if (pendingAction) {
      return;
    }

    setPendingAction(action);
    setActionError(null);
    try {
      const updated = await performNotificationAction(client, notificationId, action, {
        snoozedUntil:
          action === "snooze" ? snoozeUntilForPreset(snoozePreset) : undefined
      });
      setActionResult({ notificationId, value: updated });
      notifications.show({
        title: actionSuccessTitle(action),
        message: updated.title,
        color: "teal"
      });
    } catch (error) {
      const normalized = error instanceof Error ? error : new Error(String(error));
      setActionError(normalized);
      showError(normalized);
    } finally {
      setPendingAction(null);
    }
  }

  return (
    <ViewFrame
      eyebrow="Notification"
      title={notification?.title || "Notification detail"}
      loading={data.loading && !notification}
      loadingFallback={<PageLoader />}
      error={data.error}
      backAction={
        <Button variant="subtle" size="compact-sm" leftSection={<IconArrowLeft size={16} />} onClick={onBack}>
          Back
        </Button>
      }
    >
      {notification && (
        <Stack gap="lg" className="recordDetailPage">
          {actionError && (
            <Alert color="red" title="Notification action failed">
              {actionError.message}
            </Alert>
          )}

          <Paper p={{ base: "md", sm: "lg" }} withBorder className="recordHero">
            <Group justify="space-between" align="flex-start" gap="xl" className="recordHeroHeader">
              <Box className="recordHeroMain">
                <Group gap="xs" wrap="nowrap" mb="xs" className="recordMetaRow">
                  <StatusBadge value={notification.severity} />
                  <StatusBadge value={notification.is_read ? "read" : "unread"} />
                  <Text size="xs" c="dimmed" className="recordSource" title={notification.source}>
                    {notification.source}
                  </Text>
                </Group>
                <Text className="recordHeroTitle">{notification.title}</Text>
                <Text size="sm" c="dimmed" className="recordHeroMeta">
                  Updated {new Date(notification.updated_at).toLocaleString()}
                </Text>
              </Box>

              <Group gap="xs" wrap="wrap" className="recordHeroActions">
                {!notification.source_is_read && (
                  <Button
                    variant="light"
                    size="sm"
                    loading={pendingAction === (notification.is_read ? "unread" : "read")}
                    disabled={Boolean(pendingAction)}
                    onClick={() => runAction(notification.is_read ? "unread" : "read")}
                  >
                    {notification.is_read ? "Mark unread" : "Mark read"}
                  </Button>
                )}
                {notification.dismissed_at || notification.snoozed_until ? (
                  <Button
                    variant="light"
                    color="teal"
                    size="sm"
                    loading={pendingAction === "restore"}
                    disabled={Boolean(pendingAction)}
                    onClick={() => runAction("restore")}
                  >
                    Restore
                  </Button>
                ) : (
                  <Button
                    variant="light"
                    color="orange"
                    size="sm"
                    loading={pendingAction === "dismiss"}
                    disabled={Boolean(pendingAction)}
                    onClick={() => runAction("dismiss")}
                  >
                    Dismiss
                  </Button>
                )}
                <Select
                  aria-label="Snooze duration"
                  size="sm"
                  w={150}
                  value={snoozePreset}
                  disabled={Boolean(pendingAction)}
                  data={[
                    { label: "For 1 hour", value: "one-hour" },
                    { label: "Until tomorrow", value: "tomorrow" }
                  ]}
                  onChange={(value) => setSnoozePreset((value || "one-hour") as SnoozePreset)}
                />
                <Button
                  variant="light"
                  color="blue"
                  size="sm"
                  loading={pendingAction === "snooze"}
                  disabled={Boolean(pendingAction)}
                  onClick={() => runAction("snooze")}
                >
                  Snooze
                </Button>
                <Button
                  variant="light"
                  size="sm"
                  rightSection={<IconArrowRight size={15} />}
                  onClick={() =>
                    onOpenConnector({ source: notification.source, target: "notifications" })
                  }
                >
                  Source runs
                </Button>
                {notification.url && (
                  <Button
                    component="a"
                    href={notification.url}
                    target="_blank"
                    rel="noreferrer"
                    variant="light"
                    size="sm"
                    rightSection={<IconExternalLink size={15} />}
                  >
                    External message
                  </Button>
                )}
              </Group>
            </Group>
          </Paper>

          <Grid gutter="lg" align="flex-start">
            <Grid.Col span={{ base: 12, lg: 7 }}>
              <DataPanel title="Message">
                {notification.body ? (
                  <Text className="recordBodyText">{notification.body}</Text>
                ) : (
                  <EmptyText>No message body was imported</EmptyText>
                )}
              </DataPanel>
            </Grid.Col>

            <Grid.Col span={{ base: 12, lg: 5 }}>
              <DataPanel title="Source identity">
                <Stack gap={0} className="contextRows">
                  <ContextRow label="Record ID">
                    <Text fw={700} size="sm" ta="right">
                      #{notification.id}
                    </Text>
                  </ContextRow>
                  <ContextRow label="Severity">
                    <StatusBadge value={notification.severity} />
                  </ContextRow>
                  <ContextRow label="State">
                    <StatusBadge value={notification.is_read ? "read" : "unread"} />
                  </ContextRow>
                  <ContextRow label="Source state">
                    <StatusBadge value={notification.source_is_read ? "read" : "unread"} />
                  </ContextRow>
                  <ContextRow label="Read receipt">
                    <OptionalDate value={notification.read_at} />
                  </ContextRow>
                  <ContextRow label="Dismissed">
                    <OptionalDate value={notification.dismissed_at} />
                  </ContextRow>
                  <ContextRow label="Snoozed until">
                    <OptionalDate value={notification.snoozed_until} />
                  </ContextRow>
                  <ContextRow label="Source">
                    <Text fw={700} size="sm" ta="right" className="contextTextValue">
                      {notification.source}
                    </Text>
                  </ContextRow>
                  <ContextRow label="External ID">
                    <Text fw={700} size="sm" ta="right" className="contextTextValue">
                      {notification.external_id || "-"}
                    </Text>
                  </ContextRow>
                  <ContextRow label="Created">
                    <DateCell value={notification.created_at} />
                  </ContextRow>
                  <ContextRow label="Updated">
                    <DateCell value={notification.updated_at} />
                  </ContextRow>
                </Stack>
              </DataPanel>
            </Grid.Col>
          </Grid>

          {!notification.url && (
            <DataPanel title="External link">
              <EmptyText>No external notification URL was imported</EmptyText>
            </DataPanel>
          )}
        </Stack>
      )}
    </ViewFrame>
  );
}

function OptionalDate({ value }: { value: string | null }) {
  return value ? (
    <Text fw={700} size="sm" ta="right">
      {new Date(value).toLocaleString()}
    </Text>
  ) : (
    <Text fw={700} size="sm" ta="right" c="dimmed">
      -
    </Text>
  );
}

function actionSuccessTitle(action: NotificationAction): string {
  switch (action) {
    case "read":
      return "Marked read";
    case "unread":
      return "Marked unread";
    case "dismiss":
      return "Notification dismissed";
    case "restore":
      return "Notification restored";
    case "snooze":
      return "Notification snoozed";
  }
}

function ContextRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Group justify="space-between" align="center" wrap="nowrap" className="contextRow">
      <Text size="sm" c="dimmed">
        {label}
      </Text>
      <Box className="contextValue">{children}</Box>
    </Group>
  );
}
