import {
  Alert,
  Box,
  Button,
  Grid,
  Group,
  Paper,
  SimpleGrid,
  Stack,
  Text,
  Title
} from "@mantine/core";
import { IconAlertTriangle, IconArrowRight, IconExternalLink } from "@tabler/icons-react";

import type { ApiClient } from "../../api/client";
import { DataPanel } from "../../components/DataPanel";
import { DataTable } from "../../components/DataTable";
import { EmptyText } from "../../components/EmptyText";
import { DashboardSkeleton } from "../../components/LoadingState";
import { Metric } from "../../components/Metric";
import { QuickLinks } from "../../components/QuickLinks";
import { DateCell, StatusBadge } from "../../components/tableCells";
import { ViewFrame } from "../../components/ViewFrame";
import { useAsyncData } from "../../hooks/useAsyncData";
import { useRefresh } from "../../hooks/useRefresh";
import type {
  ConnectorDrillTarget,
  DashboardPriorityItem,
  DateTimeString,
  MeOperationsStatus,
  MeOverviewResponse,
  Notification,
  Service,
  ServiceHealthHistory
} from "../../types/api";

export function DashboardView({
  client,
  onOpenService,
  onOpenConnector
}: {
  client: ApiClient;
  onOpenService: (serviceId: string | number) => void;
  onOpenConnector: (target: ConnectorDrillTarget) => void;
}) {
  const [data, actions] = useAsyncData<MeOverviewResponse>(
    () => client.get<MeOverviewResponse>("/me/overview"),
    [client]
  );

  useRefresh(actions.reload);

  const overview = data.value;

  return (
    <ViewFrame
      eyebrow="Start the day"
      title="Morning command center"
      loading={data.loading && !overview}
      loadingFallback={<DashboardSkeleton />}
      error={data.error}
    >
      {overview && (
        <Stack gap="lg">
          <SimpleGrid cols={{ base: 1, sm: 2, lg: 5 }}>
            <Metric label="My services" value={overview.summary.services} />
            <Metric label="Today first" value={attentionCount(overview)} />
            <Metric label="Open work" value={overview.summary.open_work_cards} />
            <Metric label="Messages" value={overview.summary.unread_notifications} />
            <Metric label="Failed runs" value={overview.summary.failed_connector_runs} />
          </SimpleGrid>

          <OperationsWarning operations={overview.operations} />

          <Grid>
            <Grid.Col span={{ base: 12, lg: 6 }}>
              <DataPanel title="Today first">
                <AttentionQueue
                  overview={overview}
                  onOpenService={onOpenService}
                  onOpenConnector={onOpenConnector}
                />
              </DataPanel>
            </Grid.Col>

            <Grid.Col span={{ base: 12, lg: 6 }}>
              <DataPanel title="Health trend">
                <HealthTrendPanel history={overview.health_history} onOpenService={onOpenService} />
              </DataPanel>
            </Grid.Col>
          </Grid>

          <Grid>
            <Grid.Col span={{ base: 12, lg: 5 }}>
              <DataPanel title="Messages">
                <MessageList notifications={overview.unread_notifications} />
              </DataPanel>
            </Grid.Col>

            <Grid.Col span={{ base: 12, lg: 7 }}>
              <DataPanel title="Service health">
                <DataTable
                  rows={overview.services}
                  columns={[
                    ["name", "Service"],
                    ["health_status", "Health", StatusBadge],
                    ["lifecycle_status", "Lifecycle", StatusBadge],
                    [
                      "id",
                      "Open",
                      ({ row }) => (
                        <Button
                          size="compact-sm"
                          variant="subtle"
                          rightSection={<IconArrowRight size={14} />}
                          onClick={() => onOpenService(row.id)}
                        >
                          Overview
                        </Button>
                      )
                    ]
                  ]}
                />
              </DataPanel>
            </Grid.Col>
          </Grid>

          <Grid>
            <Grid.Col span={12}>
              <DataPanel title="Open work">
                <DataTable
                  rows={overview.open_work_cards}
                  columns={[
                    ["title", "Title"],
                    ["status", "Status", StatusBadge],
                    ["priority", "Priority", StatusBadge],
                    ["assignee", "Assignee"],
                    ["url", "Link", WorkLinkCell]
                  ]}
                />
              </DataPanel>
            </Grid.Col>
          </Grid>

          <DataPanel title="My services">
            <ServiceCards services={overview.services} onOpenService={onOpenService} />
          </DataPanel>

          <Grid>
            <Grid.Col span={{ base: 12, lg: 6 }}>
              <DataPanel title="Failed connector runs">
                <DataTable
                  rows={overview.failed_connector_runs}
                  columns={[
                    ["source", "Source"],
                    ["target", "Target"],
                    ["status", "Status", StatusBadge],
                    ["failure_count", "Failed"],
                    ["started_at", "Started", DateCell]
                  ]}
                />
              </DataPanel>
            </Grid.Col>

            <Grid.Col span={{ base: 12, lg: 6 }}>
              <DataPanel title="Packages">
                <DataTable
                  rows={overview.packages}
                  columns={[
                    ["name", "Package"],
                    ["version", "Version"],
                    ["status", "Status", StatusBadge],
                    ["repository_url", "Repo", WorkLinkCell]
                  ]}
                />
              </DataPanel>
            </Grid.Col>
          </Grid>
        </Stack>
      )}
    </ViewFrame>
  );
}

function OperationsWarning({ operations }: { operations?: MeOperationsStatus | null }) {
  if (!operations) {
    return null;
  }

  const messages: string[] = [];

  if (operations.worker_status !== "healthy") {
    messages.push(
      operations.latest_worker_seen_at
        ? `Connector worker last checked in at ${formatCheckTime(operations.latest_worker_seen_at)}`
        : "No connector worker heartbeat has been recorded"
    );
  }

  if (operations.health_data_stale) {
    messages.push(
      operations.latest_health_check_at
        ? `Service health data last updated at ${formatCheckTime(operations.latest_health_check_at)}`
        : "No service health checks are available"
    );
  }

  if (messages.length === 0) {
    return null;
  }

  return (
    <Alert
      color="yellow"
      variant="light"
      icon={<IconAlertTriangle size={18} />}
      title="Operations data may be stale"
    >
      <Stack gap={4}>
        {messages.map((message) => (
          <Text key={message} size="sm">
            {message}
          </Text>
        ))}
      </Stack>
    </Alert>
  );
}

function HealthTrendPanel({
  history,
  onOpenService
}: {
  history?: ServiceHealthHistory;
  onOpenService: (serviceId: string | number) => void;
}) {
  const summary = history?.summary;
  const checks = history?.recent_checks || [];
  const incidents = history?.recent_incidents || [];

  if (!summary || summary.checks === 0) {
    return <EmptyText>No health checks in the last day</EmptyText>;
  }

  return (
    <Stack gap="md">
      <SimpleGrid cols={{ base: 2, sm: 4 }} className="healthTrendStats">
        <TrendStat label="Checks" value={summary.checks} tone="success" />
        <TrendStat label="Down" value={summary.down_checks} tone="down" />
        <TrendStat label="Degraded" value={summary.degraded_checks} tone="degraded" />
        <TrendStat label="Changed" value={summary.changed_checks} tone="info" />
      </SimpleGrid>

      <Box className="healthTimeline" aria-label="Recent service health checks">
        {checks
          .slice(0, 24)
          .reverse()
          .map((check) => (
            <Box
              key={check.id}
              className={`healthTick is-${check.health_status}`}
              title={`${check.external_id || check.source}: ${check.health_status} at ${formatCheckTime(
                check.checked_at
              )}`}
            />
          ))}
      </Box>

      {incidents.length > 0 ? (
        <Stack gap={0} className="healthIncidentList">
          {incidents.slice(0, 4).map((check) => (
            <Group
              key={check.id}
              justify="space-between"
              align="center"
              wrap="nowrap"
              className="healthIncidentRow"
            >
              <Group gap="sm" wrap="nowrap" className="healthIncidentIdentity">
                <StatusBadge value={check.health_status} />
                <Box className="healthIncidentCopy">
                  <Text
                    fw={750}
                    className="healthIncidentTitle"
                    title={check.external_id || check.source}
                  >
                    {check.external_id || check.source}
                  </Text>
                  <Text size="sm" c="dimmed" className="healthIncidentDetail">
                    {check.source} - {formatCheckTime(check.checked_at)}
                  </Text>
                </Box>
              </Group>
              <Button
                size="compact-sm"
                variant="subtle"
                rightSection={<IconArrowRight size={14} />}
                onClick={() => onOpenService(check.service_id)}
              >
                Overview
              </Button>
            </Group>
          ))}
        </Stack>
      ) : (
        <EmptyText>No recent incidents</EmptyText>
      )}
    </Stack>
  );
}

function TrendStat({ label, value, tone }: { label: string; value: number; tone?: string }) {
  return (
    <Box className={`healthTrendStat${tone ? ` is-${tone}` : ""}`}>
      <Text size="xs" c="dimmed" fw={700} tt="uppercase">
        {label}
      </Text>
      <Text fw={850} className="healthTrendValue">
        {value}
      </Text>
    </Box>
  );
}

function MessageList({ notifications }: { notifications?: Notification[] }) {
  if (!notifications || notifications.length === 0) {
    return <EmptyText>No unread messages</EmptyText>;
  }

  return (
    <Stack gap={0} className="messageList">
      {notifications.map((notification) => (
        <Group
          key={notification.id}
          justify="space-between"
          align="center"
          wrap="nowrap"
          className="messageRow"
        >
          <Box className="messageCopy">
            <Group gap="xs" wrap="nowrap" mb={4}>
              <StatusBadge value={notification.severity} />
              <Text size="xs" c="dimmed" className="messageSource" title={notification.source}>
                {notification.source}
              </Text>
            </Group>
            <Text fw={750} className="messageTitle" title={notification.title}>
              {notification.title}
            </Text>
            {notification.body && (
              <Text size="sm" c="dimmed" className="messageBody" title={notification.body}>
                {notification.body}
              </Text>
            )}
          </Box>

          {notification.url && (
            <Button
              component="a"
              href={notification.url}
              target="_blank"
              rel="noreferrer"
              size="compact-sm"
              variant="subtle"
              rightSection={<IconExternalLink size={14} />}
            >
              Open
            </Button>
          )}
        </Group>
      ))}
    </Stack>
  );
}

function AttentionQueue({
  overview,
  onOpenService,
  onOpenConnector
}: {
  overview: MeOverviewResponse;
  onOpenService: (serviceId: string | number) => void;
  onOpenConnector: (target: ConnectorDrillTarget) => void;
}) {
  const items = overview.priority_items?.length
    ? overview.priority_items
    : buildAttentionItems(overview);

  if (items.length === 0) {
    return <EmptyText>Nothing needs attention</EmptyText>;
  }

  return (
    <Stack gap={0} className="attentionList">
      {items.map((item) => {
        const action = attentionAction(item);

        return (
          <Group key={item.key} justify="space-between" align="center" wrap="nowrap" className="attentionRow">
            <Group gap="sm" wrap="nowrap" className="attentionIdentity">
              <StatusBadge value={item.severity} />
              <Box className="attentionCopy">
                <Text fw={750} className="attentionTitle" title={item.title}>
                  {item.title}
                </Text>
                <Text size="sm" c="dimmed" className="attentionDetail" title={item.detail}>
                  {item.detail}
                </Text>
              </Box>
            </Group>

            <AttentionActionButton
              action={action}
              onOpenService={onOpenService}
              onOpenConnector={onOpenConnector}
            />
          </Group>
        );
      })}
    </Stack>
  );
}

type AttentionAction =
  | { type: "service"; label: string; serviceId: string | number }
  | { type: "connector"; label: string; target: ConnectorDrillTarget }
  | { type: "external"; label: string; url: string };

function AttentionActionButton({
  action,
  onOpenService,
  onOpenConnector
}: {
  action: AttentionAction;
  onOpenService: (serviceId: string | number) => void;
  onOpenConnector: (target: ConnectorDrillTarget) => void;
}) {
  if (action.type === "external") {
    return (
      <Button
        component="a"
        href={action.url}
        target="_blank"
        rel="noreferrer"
        size="compact-sm"
        variant="subtle"
        rightSection={<IconExternalLink size={14} />}
      >
        {action.label}
      </Button>
    );
  }

  return (
    <Button
      size="compact-sm"
      variant="subtle"
      rightSection={<IconArrowRight size={14} />}
      onClick={() =>
        action.type === "service"
          ? onOpenService(action.serviceId)
          : onOpenConnector(action.target)
      }
    >
      {action.label}
    </Button>
  );
}

function attentionAction(item: DashboardPriorityItem): AttentionAction {
  const serviceId = item.service_id ?? item.serviceId;

  if (serviceId) {
    return { type: "service", label: "Overview", serviceId };
  }

  if (item.kind === "connector_run" && item.record_id) {
    return {
      type: "connector",
      label: "Run detail",
      target: drillTarget(item, { runId: item.record_id })
    };
  }

  if (item.url) {
    return { type: "external", label: "Open", url: item.url };
  }

  if (item.source || item.target) {
    return {
      type: "connector",
      label: item.source ? "Source" : "Operations",
      target: drillTarget(item)
    };
  }

  return { type: "connector", label: "Operations", target: {} };
}

function drillTarget(
  item: DashboardPriorityItem,
  overrides: Partial<ConnectorDrillTarget> = {}
): ConnectorDrillTarget {
  return {
    source: item.source ?? undefined,
    target: item.target ?? undefined,
    ...overrides
  };
}

function attentionCount(overview: MeOverviewResponse): number {
  if (Array.isArray(overview.priority_items)) {
    return overview.priority_items.length;
  }

  return overview.summary.unhealthy_services + overview.summary.failed_connector_runs;
}

function buildAttentionItems(overview: MeOverviewResponse): DashboardPriorityItem[] {
  const operationItems: DashboardPriorityItem[] = [];
  if (overview.operations?.worker_status && overview.operations.worker_status !== "healthy") {
    operationItems.push({
      key: "operations-worker",
      kind: "worker",
      severity: overview.operations.worker_status,
      title:
        overview.operations.worker_status === "missing"
          ? "Connector worker heartbeat is missing"
          : "Connector worker heartbeat is stale",
      detail: overview.operations.latest_worker_seen_at
        ? `Last seen at ${formatCheckTime(overview.operations.latest_worker_seen_at)}`
        : "No connector worker has checked in",
      target: "worker"
    });
  }
  if (overview.operations?.health_data_stale) {
    operationItems.push({
      key: "operations-health-data",
      kind: "health_data",
      severity: "stale",
      title: "Service health data is stale",
      detail: overview.operations.latest_health_check_at
        ? `Latest health check was ${formatCheckTime(overview.operations.latest_health_check_at)}`
        : "No service health checks are available",
      target: "service_health"
    });
  }

  const services = (overview.services || [])
    .filter((service) => service.health_status !== "healthy")
    .map((service) => ({
      key: `service-${service.id}`,
      kind: "service",
      severity: service.health_status === "down" ? "down" : "degraded",
      title: service.name,
      detail: `${service.health_status} - ${service.source}`,
      source: service.source,
      target: "service_health",
      record_id: service.id,
      serviceId: service.id
    }));

  const workCards = (overview.open_work_cards || [])
    .filter((card) => card.priority === "urgent" || card.status === "blocked")
    .map((card) => ({
      key: `work-${card.id}`,
      kind: "work_card",
      severity: card.status === "blocked" ? "blocked" : "urgent",
      title: card.title,
      detail: [card.priority, card.assignee, card.source].filter(Boolean).join(" - "),
      source: card.source,
      target: "work_cards",
      record_id: card.id,
      url: card.url
    }));

  const notifications = (overview.unread_notifications || [])
    .filter((notification) => notification.severity === "critical")
    .map((notification) => ({
      key: `notification-${notification.id}`,
      kind: "notification",
      severity: notification.severity,
      title: notification.title,
      detail: notification.source,
      source: notification.source,
      target: "notifications",
      record_id: notification.id,
      url: notification.url
    }));

  const runs = (overview.failed_connector_runs || []).map((run) => ({
    key: `run-${run.id}`,
    kind: "connector_run",
    severity: run.status,
    title: `${run.source} / ${run.target}`,
    detail: run.error_message || `${run.failure_count} failed item(s)`,
    source: run.source,
    target: run.target,
    record_id: run.id
  }));

  return [...operationItems, ...services, ...workCards, ...notifications, ...runs].slice(0, 8);
}

function ServiceCards({
  services,
  onOpenService
}: {
  services?: Service[];
  onOpenService: (serviceId: string | number) => void;
}) {
  if (!services || services.length === 0) {
    return <EmptyText>No services assigned</EmptyText>;
  }

  return (
    <SimpleGrid cols={{ base: 1, md: 2, xl: 3 }}>
      {services.map((service) => (
        <Paper
          key={service.id}
          p="md"
          withBorder
          className="serviceCard"
          onClick={() => onOpenService(service.id)}
        >
          <Stack gap="sm">
            <Group justify="space-between" align="flex-start" wrap="nowrap" className="serviceCardHeader">
              <Box className="serviceCardIdentity">
                <Title order={3} size="h4" className="serviceCardTitle">
                  {service.name}
                </Title>
                <Text size="xs" c="dimmed" className="serviceCardMeta" title={`${service.slug} - ${service.source}`}>
                  {service.slug} - {service.source}
                </Text>
              </Box>
              <StatusBadge value={service.health_status} className="serviceCardHealth" />
            </Group>

            {service.description && (
              <Text size="sm" c="dimmed" lineClamp={2} className="serviceCardDescription">
                {service.description}
              </Text>
            )}

            <Group justify="space-between" align="flex-end" gap="sm" className="serviceCardFooter">
              <Box className="serviceCardLifecycle">
                <StatusBadge value={service.lifecycle_status} />
              </Box>
              <QuickLinks links={service} />
            </Group>
          </Stack>
        </Paper>
      ))}
    </SimpleGrid>
  );
}

function formatCheckTime(value?: DateTimeString | null): string {
  if (!value) {
    return "";
  }

  return new Date(value).toLocaleString();
}

function WorkLinkCell({ value }: { value?: unknown }) {
  if (!value) {
    return null;
  }

  return (
    <Button
      component="a"
      href={String(value)}
      target="_blank"
      rel="noreferrer"
      size="compact-sm"
      variant="subtle"
      rightSection={<IconExternalLink size={14} />}
    >
      Open
    </Button>
  );
}
