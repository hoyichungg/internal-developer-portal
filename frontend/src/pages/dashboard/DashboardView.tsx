import {
  Alert,
  Box,
  Button,
  Grid,
  Group,
  Paper,
  SegmentedControl,
  Select,
  SimpleGrid,
  Stack,
  Text,
  TextInput,
  Title
} from "@mantine/core";
import {
  IconAlertTriangle,
  IconArrowRight,
  IconExternalLink,
  IconFilter,
  IconSearch,
  IconSortDescending
} from "@tabler/icons-react";
import { useMemo, useState } from "react";

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
  onOpenConnector,
  onOpenWorkCard,
  onOpenNotification
}: {
  client: ApiClient;
  onOpenService: (serviceId: string | number) => void;
  onOpenConnector: (target: ConnectorDrillTarget) => void;
  onOpenWorkCard: (workCardId: string | number) => void;
  onOpenNotification: (notificationId: string | number) => void;
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
            <Grid.Col span={{ base: 12, lg: 7 }}>
              <DataPanel title="Daily workbench">
                <DailyWorkbench
                  overview={overview}
                  onOpenService={onOpenService}
                  onOpenConnector={onOpenConnector}
                  onOpenWorkCard={onOpenWorkCard}
                  onOpenNotification={onOpenNotification}
                />
              </DataPanel>
            </Grid.Col>

            <Grid.Col span={{ base: 12, lg: 5 }}>
              <DataPanel title="Health trend">
                <HealthTrendPanel history={overview.health_history} onOpenService={onOpenService} />
              </DataPanel>
            </Grid.Col>
          </Grid>

          <Grid>
            <Grid.Col span={{ base: 12, lg: 5 }}>
              <DataPanel title="Messages">
                <MessageList
                  notifications={overview.unread_notifications}
                  onOpenNotification={onOpenNotification}
                />
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
                    [
                      "id",
                      "Details",
                      ({ row }) => (
                        <Button
                          size="compact-sm"
                          variant="subtle"
                          rightSection={<IconArrowRight size={14} />}
                          onClick={() => onOpenWorkCard(row.id)}
                        >
                          Details
                        </Button>
                      )
                    ],
                    ["url", "External", WorkLinkCell]
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

type WorkbenchFilter =
  | "all"
  | "service"
  | "work_card"
  | "notification"
  | "connector_run"
  | "operations";
type WorkbenchSort = "impact" | "recent" | "source";

type WorkbenchItem = {
  key: string;
  kind: Exclude<WorkbenchFilter, "all">;
  kindLabel: string;
  severity: string;
  title: string;
  detail: string;
  source?: string | null;
  target?: string | null;
  occurredAt?: DateTimeString | null;
  rank: number;
  action: AttentionAction;
  url?: string | null;
};

const workbenchFilters: { label: string; value: WorkbenchFilter }[] = [
  { label: "All", value: "all" },
  { label: "Services", value: "service" },
  { label: "Work", value: "work_card" },
  { label: "Messages", value: "notification" },
  { label: "Runs", value: "connector_run" },
  { label: "Ops", value: "operations" }
];

const workbenchSorts: { label: string; value: WorkbenchSort }[] = [
  { label: "Impact first", value: "impact" },
  { label: "Newest first", value: "recent" },
  { label: "Source", value: "source" }
];

function DailyWorkbench({
  overview,
  onOpenService,
  onOpenConnector,
  onOpenWorkCard,
  onOpenNotification
}: {
  overview: MeOverviewResponse;
  onOpenService: (serviceId: string | number) => void;
  onOpenConnector: (target: ConnectorDrillTarget) => void;
  onOpenWorkCard: (workCardId: string | number) => void;
  onOpenNotification: (notificationId: string | number) => void;
}) {
  const [kindFilter, setKindFilter] = useState<WorkbenchFilter>("all");
  const [sortMode, setSortMode] = useState<WorkbenchSort>("impact");
  const [query, setQuery] = useState("");
  const allItems = useMemo(() => buildWorkbenchItems(overview), [overview]);
  const visibleItems = useMemo(
    () => sortWorkbenchItems(filterWorkbenchItems(allItems, kindFilter, query), sortMode),
    [allItems, kindFilter, query, sortMode]
  );
  const summary = useMemo(() => summarizeWorkbenchItems(allItems), [allItems]);

  return (
    <Box component="section" aria-label="Daily workbench" className="workbenchSurface">
      <Stack gap="md">
        <SimpleGrid cols={{ base: 2, sm: 4 }} className="workbenchStats">
          <WorkbenchStat label="Critical" value={summary.critical} tone="critical" />
          <WorkbenchStat label="Work" value={summary.work} tone="work" />
          <WorkbenchStat label="Messages" value={summary.messages} tone="messages" />
          <WorkbenchStat label="Runs" value={summary.runs} tone="runs" />
        </SimpleGrid>

        <Group gap="sm" align="flex-end" className="workbenchToolbar">
          <SegmentedControl
            aria-label="Filter workbench"
            size="sm"
            value={kindFilter}
            data={workbenchFilters}
            onChange={(value) => setKindFilter(value as WorkbenchFilter)}
            className="workbenchFilter"
          />
          <Select
            aria-label="Sort workbench"
            size="sm"
            value={sortMode}
            data={workbenchSorts}
            leftSection={<IconSortDescending size={16} />}
            onChange={(value) => setSortMode((value as WorkbenchSort) || "impact")}
            className="workbenchSort"
          />
          <TextInput
            aria-label="Search workbench"
            size="sm"
            value={query}
            placeholder="Search"
            leftSection={<IconSearch size={16} />}
            onChange={(event) => setQuery(event.currentTarget.value)}
            className="workbenchSearch"
          />
        </Group>

        <Group justify="space-between" gap="sm" className="workbenchResultMeta">
          <Group gap={6} wrap="nowrap">
            <IconFilter size={15} />
            <Text size="sm" c="dimmed">
              {visibleItems.length} of {allItems.length}
            </Text>
          </Group>
          {query && (
            <Button size="compact-sm" variant="subtle" onClick={() => setQuery("")}>
              Clear
            </Button>
          )}
        </Group>

        {visibleItems.length > 0 ? (
          <Stack gap={0} className="workbenchList">
            {visibleItems.slice(0, 12).map((item) => (
              <Group
                key={item.key}
                data-testid="workbench-row"
                justify="space-between"
                align="center"
                wrap="nowrap"
                className={`workbenchRow is-${item.kind}`}
              >
                <Group gap="sm" wrap="nowrap" className="workbenchIdentity">
                  <StatusBadge value={item.severity} />
                  <Box className="workbenchCopy">
                    <Group gap="xs" wrap="nowrap" className="workbenchTitleRow">
                      <Text fw={760} className="workbenchTitle" title={item.title}>
                        {item.title}
                      </Text>
                      <Text size="xs" c="dimmed" className="workbenchKind">
                        {item.kindLabel}
                      </Text>
                    </Group>
                    <Text size="sm" c="dimmed" className="workbenchDetail" title={item.detail}>
                      {item.detail}
                    </Text>
                    <Group gap={6} className="workbenchMeta">
                      {item.source && (
                        <Text size="xs" c="dimmed" className="workbenchMetaText">
                          {item.source}
                        </Text>
                      )}
                      {item.target && (
                        <Text size="xs" c="dimmed" className="workbenchMetaText">
                          {item.target}
                        </Text>
                      )}
                      {item.occurredAt && (
                        <Text size="xs" c="dimmed" className="workbenchMetaText">
                          {formatCheckTime(item.occurredAt)}
                        </Text>
                      )}
                    </Group>
                  </Box>
                </Group>

                <AttentionActionButton
                  action={item.action}
                  itemTitle={item.title}
                  onOpenService={onOpenService}
                  onOpenConnector={onOpenConnector}
                  onOpenWorkCard={onOpenWorkCard}
                  onOpenNotification={onOpenNotification}
                />
              </Group>
            ))}
          </Stack>
        ) : (
          <EmptyText>No matching workbench items</EmptyText>
        )}
      </Stack>
    </Box>
  );
}

function WorkbenchStat({
  label,
  value,
  tone
}: {
  label: string;
  value: number;
  tone: string;
}) {
  return (
    <Box className={`workbenchStat is-${tone}`}>
      <Text size="xs" c="dimmed" fw={700} tt="uppercase">
        {label}
      </Text>
      <Text fw={850} className="workbenchStatValue">
        {value}
      </Text>
    </Box>
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

function MessageList({
  notifications,
  onOpenNotification
}: {
  notifications?: Notification[];
  onOpenNotification: (notificationId: string | number) => void;
}) {
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

          <Group gap="xs" wrap="nowrap" className="messageActions">
            <Button
              size="compact-sm"
              variant="subtle"
              rightSection={<IconArrowRight size={14} />}
              onClick={() => onOpenNotification(notification.id)}
            >
              Details
            </Button>
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
        </Group>
      ))}
    </Stack>
  );
}

type AttentionAction =
  | { type: "service"; label: string; serviceId: string | number }
  | { type: "connector"; label: string; target: ConnectorDrillTarget }
  | { type: "work-card"; label: string; workCardId: string | number }
  | { type: "notification"; label: string; notificationId: string | number }
  | { type: "external"; label: string; url: string };

function AttentionActionButton({
  action,
  itemTitle,
  onOpenService,
  onOpenConnector,
  onOpenWorkCard,
  onOpenNotification
}: {
  action: AttentionAction;
  itemTitle: string;
  onOpenService: (serviceId: string | number) => void;
  onOpenConnector: (target: ConnectorDrillTarget) => void;
  onOpenWorkCard: (workCardId: string | number) => void;
  onOpenNotification: (notificationId: string | number) => void;
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
        aria-label={`${action.label} ${itemTitle}`}
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
      aria-label={`${action.label} ${itemTitle}`}
      rightSection={<IconArrowRight size={14} />}
      onClick={() =>
        action.type === "service"
          ? onOpenService(action.serviceId)
          : action.type === "work-card"
            ? onOpenWorkCard(action.workCardId)
            : action.type === "notification"
              ? onOpenNotification(action.notificationId)
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

  if (item.kind === "work_card" && item.record_id) {
    return { type: "work-card", label: "Details", workCardId: item.record_id };
  }

  if (item.kind === "notification" && item.record_id) {
    return { type: "notification", label: "Details", notificationId: item.record_id };
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

function buildWorkbenchItems(overview: MeOverviewResponse): WorkbenchItem[] {
  const items = new Map<string, WorkbenchItem>();
  const put = (item: WorkbenchItem) => {
    const existing = items.get(item.key);

    if (!existing) {
      items.set(item.key, item);
      return;
    }

    if (item.rank < existing.rank) {
      items.set(item.key, {
        ...item,
        occurredAt: item.occurredAt || existing.occurredAt,
        url: item.url || existing.url
      });
      return;
    }

    if (!existing.occurredAt || !existing.url) {
      items.set(item.key, {
        ...existing,
        occurredAt: existing.occurredAt || item.occurredAt,
        url: existing.url || item.url
      });
    }
  };

  const priorityItems = overview.priority_items?.length
    ? overview.priority_items
    : buildAttentionItems(overview);
  priorityItems.forEach((item) => put(workbenchItemFromPriority(item)));

  (overview.services || [])
    .filter((service) => service.health_status !== "healthy")
    .forEach((service) =>
      put({
        key: `service-${service.id}`,
        kind: "service",
        kindLabel: "Service",
        severity: service.health_status,
        title: service.name,
        detail: `${service.health_status} service from ${service.source}`,
        source: service.source,
        target: "service_health",
        occurredAt: service.last_checked_at || service.updated_at,
        rank: service.health_status === "down" ? 10 : 20,
        action: { type: "service", label: "Overview", serviceId: service.id },
        url: service.dashboard_url || service.runbook_url
      })
    );

  (overview.open_work_cards || []).forEach((card) =>
    put({
      key: `work-card-${card.id}`,
      kind: "work_card",
      kindLabel: "Work",
      severity: workCardSeverity(card.status, card.priority),
      title: card.title,
      detail: [card.status, card.priority, card.assignee].filter(Boolean).join(" - "),
      source: card.source,
      target: "work_cards",
      occurredAt: card.updated_at || card.created_at,
      rank: workCardRank(card.status, card.priority),
      action: { type: "work-card", label: "Details", workCardId: card.id },
      url: card.url
    })
  );

  (overview.unread_notifications || []).forEach((notification) =>
    put({
      key: `notification-${notification.id}`,
      kind: "notification",
      kindLabel: "Message",
      severity: notification.severity,
      title: notification.title,
      detail: notification.body || notification.source,
      source: notification.source,
      target: "notifications",
      occurredAt: notification.updated_at || notification.created_at,
      rank: notificationRank(notification.severity),
      action: {
        type: "notification",
        label: "Details",
        notificationId: notification.id
      },
      url: notification.url
    })
  );

  (overview.failed_connector_runs || []).forEach((run) =>
    put({
      key: `connector-run-${run.id}`,
      kind: "connector_run",
      kindLabel: "Run",
      severity: run.status,
      title: `${run.source} / ${run.target}`,
      detail: run.error_message || `${run.failure_count} failed item(s)`,
      source: run.source,
      target: run.target,
      occurredAt: run.finished_at || run.started_at,
      rank: run.status === "failed" ? 60 : 65,
      action: {
        type: "connector",
        label: "Run detail",
        target: { source: run.source, target: run.target, runId: run.id }
      }
    })
  );

  return sortWorkbenchItems(Array.from(items.values()), "impact");
}

function workbenchItemFromPriority(item: DashboardPriorityItem): WorkbenchItem {
  const kind = workbenchKindFromPriority(item);

  return {
    key: workbenchKeyFromPriority(item),
    kind,
    kindLabel: workbenchKindLabel(kind),
    severity: item.severity || "info",
    title: item.title,
    detail: item.detail,
    source: item.source,
    target: item.target,
    occurredAt: item.occurred_at,
    rank: typeof item.rank === "number" ? item.rank : impactRank(item.severity),
    action: attentionAction(item),
    url: item.url
  };
}

function workbenchKeyFromPriority(item: DashboardPriorityItem): string {
  const serviceId = item.service_id ?? item.serviceId;

  if (serviceId) {
    return `service-${serviceId}`;
  }
  if (item.kind === "connector_run" && item.record_id) {
    return `connector-run-${item.record_id}`;
  }
  if (item.kind === "work_card" && item.record_id) {
    return `work-card-${item.record_id}`;
  }
  if (item.kind === "notification" && item.record_id) {
    return `notification-${item.record_id}`;
  }

  return `priority-${item.key}`;
}

function workbenchKindFromPriority(item: DashboardPriorityItem): WorkbenchItem["kind"] {
  if (item.service_id || item.serviceId || item.kind === "service") {
    return "service";
  }
  if (item.kind === "work_card") {
    return "work_card";
  }
  if (item.kind === "notification") {
    return "notification";
  }
  if (item.kind === "connector_run") {
    return "connector_run";
  }

  return "operations";
}

function workbenchKindLabel(kind: WorkbenchItem["kind"]): string {
  switch (kind) {
    case "service":
      return "Service";
    case "work_card":
      return "Work";
    case "notification":
      return "Message";
    case "connector_run":
      return "Run";
    case "operations":
      return "Ops";
  }
}

function filterWorkbenchItems(
  items: WorkbenchItem[],
  kindFilter: WorkbenchFilter,
  query: string
): WorkbenchItem[] {
  const needle = query.trim().toLowerCase();

  return items.filter((item) => {
    if (kindFilter !== "all" && item.kind !== kindFilter) {
      return false;
    }
    if (!needle) {
      return true;
    }

    return [
      item.title,
      item.detail,
      item.source,
      item.target,
      item.kindLabel,
      item.severity
    ]
      .filter(Boolean)
      .some((value) => String(value).toLowerCase().includes(needle));
  });
}

function sortWorkbenchItems(items: WorkbenchItem[], sortMode: WorkbenchSort): WorkbenchItem[] {
  return [...items].sort((left, right) => {
    if (sortMode === "recent") {
      return (
        timeValue(right.occurredAt) - timeValue(left.occurredAt) ||
        left.rank - right.rank ||
        left.title.localeCompare(right.title)
      );
    }

    if (sortMode === "source") {
      return (
        String(left.source || "").localeCompare(String(right.source || "")) ||
        left.rank - right.rank ||
        left.title.localeCompare(right.title)
      );
    }

    return (
      left.rank - right.rank ||
      timeValue(right.occurredAt) - timeValue(left.occurredAt) ||
      left.title.localeCompare(right.title)
    );
  });
}

function summarizeWorkbenchItems(items: WorkbenchItem[]) {
  return {
    critical: items.filter((item) => isCriticalWorkbenchItem(item)).length,
    work: items.filter((item) => item.kind === "work_card").length,
    messages: items.filter((item) => item.kind === "notification").length,
    runs: items.filter((item) => item.kind === "connector_run").length
  };
}

function isCriticalWorkbenchItem(item: WorkbenchItem): boolean {
  return ["blocked", "critical", "down", "error", "failed", "urgent"].includes(
    item.severity.toLowerCase()
  );
}

function workCardSeverity(status: string, priority: string): string {
  if (status === "blocked") {
    return "blocked";
  }
  return priority || status;
}

function workCardRank(status: string, priority: string): number {
  if (status === "blocked") {
    return 30;
  }

  switch (priority) {
    case "urgent":
      return 40;
    case "high":
      return 45;
    case "medium":
      return 55;
    default:
      return 70;
  }
}

function notificationRank(severity: string): number {
  switch (severity) {
    case "critical":
      return 50;
    case "warning":
      return 55;
    default:
      return 80;
  }
}

function impactRank(severity: string): number {
  switch (severity) {
    case "down":
      return 10;
    case "degraded":
      return 20;
    case "blocked":
      return 30;
    case "urgent":
      return 40;
    case "critical":
      return 50;
    case "failed":
      return 60;
    case "partial_success":
      return 65;
    case "warning":
    case "stale":
    case "missing":
      return 70;
    default:
      return 80;
  }
}

function timeValue(value?: DateTimeString | null): number {
  if (!value) {
    return 0;
  }

  const time = Date.parse(value);
  return Number.isFinite(time) ? time : 0;
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
