import { Box, Button, Grid, Group, Paper, Stack, Text } from "@mantine/core";
import { IconArrowLeft } from "@tabler/icons-react";

import { DataPanel } from "../../components/DataPanel.jsx";
import { DataTable } from "../../components/DataTable.jsx";
import { EmptyText } from "../../components/EmptyText.jsx";
import { ServiceOverviewSkeleton } from "../../components/LoadingState.jsx";
import { QuickLinks } from "../../components/QuickLinks.jsx";
import { DateCell, StatusBadge } from "../../components/tableCells.jsx";
import { ViewFrame } from "../../components/ViewFrame.jsx";
import { useAsyncData } from "../../hooks/useAsyncData.js";
import { useRefresh } from "../../hooks/useRefresh.js";

export function ServiceOverviewView({ client, serviceId, onBack }) {
  const [data, actions] = useAsyncData(
    () => client.get(`/services/${encodeURIComponent(serviceId)}/overview`),
    [client, serviceId]
  );

  useRefresh(actions.reload);

  const overview = data.value;
  const service = overview?.service;

  return (
    <ViewFrame
      eyebrow="Service"
      title={service?.name || "Service overview"}
      loading={data.loading && !overview}
      loadingFallback={<ServiceOverviewSkeleton />}
      error={data.error}
      backAction={
        <Button variant="subtle" size="compact-sm" leftSection={<IconArrowLeft size={16} />} onClick={onBack}>
          Back
        </Button>
      }
    >
      {overview && (
        <Stack gap="lg" className="serviceOverviewPage">
          <Paper p={{ base: "md", sm: "lg" }} withBorder className="serviceOverviewSummary">
            <Group justify="space-between" align="flex-start" gap="xl" className="serviceSummaryHeader">
              <Box className="serviceSummaryMain">
                <Group gap="xs" wrap="nowrap" className="serviceMetaRow">
                  <Text
                    size="sm"
                    c="dimmed"
                    className="serviceSlug"
                    title={`${overview.service.slug} - ${overview.service.source}`}
                  >
                    {overview.service.slug}
                  </Text>
                  <Text size="xs" c="dimmed" className="serviceSource">
                    {overview.service.source}
                  </Text>
                </Group>
                <Text className="serviceOverviewDescription">
                  {overview.service.description || "No service description configured."}
                </Text>
              </Box>

              <Stack gap="sm" align="flex-end" className="serviceSummaryActions">
                <Group gap="xs" wrap="nowrap">
                  <StatusBadge value={overview.health.status} />
                  <StatusBadge value={overview.health.lifecycle_status} />
                </Group>
                <QuickLinks links={overview.links} compact={false} />
                {!hasAnyLink(overview.links) && <EmptyText>No links configured</EmptyText>}
              </Stack>
            </Group>
          </Paper>

          <Grid gutter="lg" align="flex-start">
            <Grid.Col span={{ base: 12, lg: 8 }}>
              <Stack gap="lg">
                <DataPanel title="Packages">
                  <DataTable
                    rows={overview.packages}
                    columns={[
                      ["name", "Package"],
                      ["version", "Version"],
                      ["status", "Status", StatusBadge],
                      ["repository_url", "Repo", RepoCell]
                    ]}
                  />
                </DataPanel>

                <DataPanel title="Recent connector runs">
                  <DataTable
                    rows={overview.recent_connector_runs}
                    columns={[
                      ["source", "Source"],
                      ["target", "Target"],
                      ["status", "Status", StatusBadge],
                      ["success_count", "OK"],
                      ["failure_count", "Failed"],
                      ["started_at", "Started", DateCell]
                    ]}
                  />
                </DataPanel>
              </Stack>
            </Grid.Col>

            <Grid.Col span={{ base: 12, lg: 4 }}>
              <Stack gap="lg" className="serviceOverviewSidebar">
                <Paper p={{ base: "md", sm: "lg" }} withBorder className="serviceSideCard">
                  <Text fw={800} size="lg">
                    Service context
                  </Text>
                  <Stack gap={0} mt="md" className="contextRows">
                    <ContextRow label="Health">
                      <StatusBadge value={overview.health.status} />
                    </ContextRow>
                    <ContextRow label="Lifecycle">
                      <StatusBadge value={overview.health.lifecycle_status} />
                    </ContextRow>
                    <ContextRow label="Last checked">
                      <Text fw={700} size="sm" ta="right">
                        {formatDate(overview.health.last_checked_at)}
                      </Text>
                    </ContextRow>
                    <ContextRow label="Packages">
                      <Text fw={700} size="sm" ta="right" className="contextTextValue">
                        {overview.packages.length}
                      </Text>
                    </ContextRow>
                    <ContextRow label="Recent runs">
                      <Text fw={700} size="sm" ta="right" className="contextTextValue">
                        {overview.recent_connector_runs.length}
                      </Text>
                    </ContextRow>
                  </Stack>
                </Paper>

                <Paper p={{ base: "md", sm: "lg" }} withBorder className="serviceSideCard">
                  <Text fw={800} size="lg">
                    Ownership
                  </Text>
                  <Stack gap="md" mt="md">
                    <PersonBlock
                      label="Owner"
                      name={overview.owner?.display_name}
                      email={overview.owner?.email}
                    />
                    <PersonBlock
                      label="Maintainer"
                      name={overview.maintainer?.display_name}
                      email={overview.maintainer?.email}
                    />
                    <Box>
                      <Text size="xs" c="dimmed" fw={700} tt="uppercase" mb="xs">
                        Members
                      </Text>
                      <MemberList members={overview.maintainer_members} />
                    </Box>
                  </Stack>
                </Paper>
              </Stack>
            </Grid.Col>
          </Grid>
        </Stack>
      )}
    </ViewFrame>
  );
}

function ContextRow({ label, children }) {
  return (
    <Group justify="space-between" align="center" wrap="nowrap" className="contextRow">
      <Text size="sm" c="dimmed">
        {label}
      </Text>
      <Box className="contextValue">{children}</Box>
    </Group>
  );
}

function PersonBlock({ label, name, email }) {
  return (
    <Box className="personBlock">
      <Text size="xs" c="dimmed" fw={700} tt="uppercase">
        {label}
      </Text>
      <Text fw={700}>{name || "-"}</Text>
      {email && (
        <Text size="sm" c="dimmed">
          {email}
        </Text>
      )}
    </Box>
  );
}

function MemberList({ members }) {
  if (!members || members.length === 0) {
    return <EmptyText>No members</EmptyText>;
  }

  return (
    <Stack gap="xs" className="memberList">
      {members.map((member) => (
        <Group
          key={member.id || `${member.user_id}-${member.role}`}
          justify="space-between"
          wrap="nowrap"
          className="memberRow"
        >
          <Text size="sm" fw={700}>
            User {member.user_id}
          </Text>
          <StatusBadge value={member.role} />
        </Group>
      ))}
    </Stack>
  );
}

function RepoCell({ value }) {
  if (!value) {
    return null;
  }

  return (
    <Button
      component="a"
      href={value}
      target="_blank"
      rel="noreferrer"
      size="compact-sm"
      variant="subtle"
    >
      Open
    </Button>
  );
}

function formatDate(value) {
  if (!value) {
    return "-";
  }

  return new Date(value).toLocaleString();
}

function hasAnyLink(links) {
  return Boolean(links?.repository_url || links?.dashboard_url || links?.runbook_url);
}
