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
import type { ApiId, ConnectorDrillTarget, DateTimeString, WorkCard } from "../../types/api";

export function WorkCardDetailView({
  client,
  workCardId,
  onBack,
  onOpenConnector
}: {
  client: ApiClient;
  workCardId: ApiId;
  onBack: () => void;
  onOpenConnector: (target: ConnectorDrillTarget) => void;
}) {
  const [data, actions] = useAsyncData<WorkCard>(
    () => client.get<WorkCard>(`/work-cards/${encodeURIComponent(String(workCardId))}`),
    [client, workCardId]
  );

  useRefresh(actions.reload);

  const card = data.value;

  return (
    <ViewFrame
      eyebrow="Work card"
      title={card?.title || "Work card detail"}
      loading={data.loading && !card}
      loadingFallback={<PageLoader />}
      error={data.error}
      backAction={
        <Button variant="subtle" size="compact-sm" leftSection={<IconArrowLeft size={16} />} onClick={onBack}>
          Back
        </Button>
      }
    >
      {card && (
        <Stack gap="lg" className="recordDetailPage">
          <Paper p={{ base: "md", sm: "lg" }} withBorder className="recordHero">
            <Group justify="space-between" align="flex-start" gap="xl" className="recordHeroHeader">
              <Box className="recordHeroMain">
                <Group gap="xs" wrap="nowrap" mb="xs" className="recordMetaRow">
                  <StatusBadge value={card.status} />
                  <StatusBadge value={card.priority} />
                  <Text size="xs" c="dimmed" className="recordSource" title={card.source}>
                    {card.source}
                  </Text>
                </Group>
                <Text className="recordHeroTitle">{card.title}</Text>
                <Text size="sm" c="dimmed" className="recordHeroMeta">
                  {card.assignee ? `Assigned to ${card.assignee}` : "No assignee"} ·{" "}
                  {card.due_at ? `Due ${formatDate(card.due_at)}` : "No due date"}
                </Text>
              </Box>

              <Group gap="xs" wrap="wrap" className="recordHeroActions">
                <Button
                  variant="light"
                  size="sm"
                  rightSection={<IconArrowRight size={15} />}
                  onClick={() => onOpenConnector({ source: card.source, target: "work_cards" })}
                >
                  Source runs
                </Button>
                {card.url && (
                  <Button
                    component="a"
                    href={card.url}
                    target="_blank"
                    rel="noreferrer"
                    variant="light"
                    size="sm"
                    rightSection={<IconExternalLink size={15} />}
                  >
                    External card
                  </Button>
                )}
              </Group>
            </Group>
          </Paper>

          <Grid gutter="lg" align="flex-start">
            <Grid.Col span={{ base: 12, lg: 7 }}>
              <DataPanel title="Work context">
                <Stack gap={0} className="contextRows">
                  <ContextRow label="Title">
                    <Text fw={700} size="sm" ta="right" className="contextTextValue">
                      {card.title}
                    </Text>
                  </ContextRow>
                  <ContextRow label="Status">
                    <StatusBadge value={card.status} />
                  </ContextRow>
                  <ContextRow label="Priority">
                    <StatusBadge value={card.priority} />
                  </ContextRow>
                  <ContextRow label="Assignee">
                    <Text fw={700} size="sm" ta="right" className="contextTextValue">
                      {card.assignee || "-"}
                    </Text>
                  </ContextRow>
                  <ContextRow label="Due">
                    <Text fw={700} size="sm" ta="right">
                      {formatDate(card.due_at)}
                    </Text>
                  </ContextRow>
                </Stack>
              </DataPanel>
            </Grid.Col>

            <Grid.Col span={{ base: 12, lg: 5 }}>
              <DataPanel title="Source identity">
                <Stack gap={0} className="contextRows">
                  <ContextRow label="Record ID">
                    <Text fw={700} size="sm" ta="right">
                      #{card.id}
                    </Text>
                  </ContextRow>
                  <ContextRow label="Source">
                    <Text fw={700} size="sm" ta="right" className="contextTextValue">
                      {card.source}
                    </Text>
                  </ContextRow>
                  <ContextRow label="External ID">
                    <Text fw={700} size="sm" ta="right" className="contextTextValue">
                      {card.external_id || "-"}
                    </Text>
                  </ContextRow>
                  <ContextRow label="Created">
                    <DateCell value={card.created_at} />
                  </ContextRow>
                  <ContextRow label="Updated">
                    <DateCell value={card.updated_at} />
                  </ContextRow>
                </Stack>
              </DataPanel>
            </Grid.Col>
          </Grid>

          {!card.url && (
            <DataPanel title="External link">
              <EmptyText>No external work card URL was imported</EmptyText>
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

function formatDate(value?: DateTimeString | null): string {
  if (!value) {
    return "-";
  }

  return new Date(value).toLocaleString();
}
