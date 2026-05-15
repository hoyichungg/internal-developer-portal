import { Box, Button, Grid, Group, Paper, Stack, Text } from "@mantine/core";
import { IconArrowLeft, IconArrowRight, IconExternalLink } from "@tabler/icons-react";
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

  useRefresh(actions.reload);

  const notification = data.value;

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
