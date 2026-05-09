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

import { DataPanel } from "../../components/DataPanel.jsx";
import { DataTable } from "../../components/DataTable.jsx";
import { EmptyText } from "../../components/EmptyText.jsx";
import { DashboardSkeleton } from "../../components/LoadingState.jsx";
import { Metric } from "../../components/Metric.jsx";
import { QuickLinks } from "../../components/QuickLinks.jsx";
import { DateCell, StatusBadge } from "../../components/tableCells.jsx";
import { ViewFrame } from "../../components/ViewFrame.jsx";
import { useAsyncData } from "../../hooks/useAsyncData.js";
import { useRefresh } from "../../hooks/useRefresh.js";

export function DashboardView({ client, onOpenService }) {
  const [data, actions] = useAsyncData(() => client.get("/me/overview"), [client]);

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
            <Metric label="Needs attention" value={overview.summary.unhealthy_services} />
            <Metric label="Open work" value={overview.summary.open_work_cards} />
            <Metric label="Messages" value={overview.summary.unread_notifications} />
            <Metric label="Failed runs" value={overview.summary.failed_connector_runs} />
          </SimpleGrid>

          <OperationsWarning operations={overview.operations} />

          <Grid>
            <Grid.Col span={{ base: 12, lg: 6 }}>
              <DataPanel title="Needs attention">
                <AttentionQueue overview={overview} onOpenService={onOpenService} />
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

function OperationsWarning({ operations }) {
  if (!operations) {
    return null;
  }

  const messages = [];

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

function HealthTrendPanel({ history, onOpenService }) {
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

function TrendStat({ label, value, tone }) {
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

function MessageList({ notifications }) {
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

function AttentionQueue({ overview, onOpenService }) {
  const items = buildAttentionItems(overview);

  if (items.length === 0) {
    return <EmptyText>Nothing needs attention</EmptyText>;
  }

  return (
    <Stack gap={0} className="attentionList">
      {items.map((item) => (
        <Group
          key={item.key}
          justify="space-between"
          align="center"
          wrap="nowrap"
          className="attentionRow"
        >
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

          {item.serviceId && (
            <Button
              size="compact-sm"
              variant="subtle"
              rightSection={<IconArrowRight size={14} />}
              onClick={() => onOpenService(item.serviceId)}
            >
              Overview
            </Button>
          )}
          {item.url && (
            <Button
              component="a"
              href={item.url}
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

function buildAttentionItems(overview) {
  const services = (overview.services || [])
    .filter((service) => service.health_status !== "healthy")
    .map((service) => ({
      key: `service-${service.id}`,
      severity: service.health_status === "down" ? "down" : "degraded",
      title: service.name,
      detail: `${service.health_status} - ${service.source}`,
      serviceId: service.id
    }));

  const workCards = (overview.open_work_cards || [])
    .filter((card) => card.priority === "urgent" || card.status === "blocked")
    .map((card) => ({
      key: `work-${card.id}`,
      severity: card.status === "blocked" ? "blocked" : "urgent",
      title: card.title,
      detail: [card.priority, card.assignee, card.source].filter(Boolean).join(" - "),
      url: card.url
    }));

  const notifications = (overview.unread_notifications || [])
    .filter((notification) => ["critical", "warning"].includes(notification.severity))
    .map((notification) => ({
      key: `notification-${notification.id}`,
      severity: notification.severity,
      title: notification.title,
      detail: notification.source,
      url: notification.url
    }));

  const runs = (overview.failed_connector_runs || []).map((run) => ({
    key: `run-${run.id}`,
    severity: run.status,
    title: `${run.source} / ${run.target}`,
    detail: run.error_message || `${run.failure_count} failed item(s)`
  }));

  return [...services, ...workCards, ...notifications, ...runs].slice(0, 8);
}

function ServiceCards({ services, onOpenService }) {
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

function formatCheckTime(value) {
  if (!value) {
    return "";
  }

  return new Date(value).toLocaleString();
}

function WorkLinkCell({ value }) {
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
      rightSection={<IconExternalLink size={14} />}
    >
      Open
    </Button>
  );
}
