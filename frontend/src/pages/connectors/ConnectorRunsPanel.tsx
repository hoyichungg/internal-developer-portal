import {
  Box,
  Button,
  Code,
  Group,
  Loader,
  Modal,
  SimpleGrid,
  Stack,
  Text
} from "@mantine/core";
import { IconArrowRight, IconEye, IconRefresh } from "@tabler/icons-react";
import { useState } from "react";
import type { ReactNode } from "react";

import { DataPanel } from "../../components/DataPanel";
import { DataTable } from "../../components/DataTable";
import { EmptyText } from "../../components/EmptyText";
import { DateCell, StatusBadge } from "../../components/tableCells";
import { prettyJson } from "../../utils/format";

export function ConnectorRunsPanel({
  runs,
  runDetail,
  runDetailLoading,
  onSelectRun,
  onRetryRun,
  retryingRunId,
  onOpenService
}) {
  return (
    <Stack gap="md">
      <DataPanel title="Recent runs">
        <DataTable
          rows={runs}
          columns={[
            ["id", "ID"],
            ["target", "Target"],
            ["trigger", "Trigger"],
            ["status", "Status", StatusBadge],
            ["success_count", "OK"],
            ["failure_count", "Failed"],
            ["duration_ms", "MS"],
            ["started_at", "Started", DateCell],
            [
              "id",
              "Retry",
              ({ row }) =>
                canRetry(row) ? (
                  <Button
                    size="compact-sm"
                    variant="subtle"
                    leftSection={<IconRefresh size={14} />}
                    loading={retryingRunId === row.id}
                    onClick={() => onRetryRun(row)}
                  >
                    Retry
                  </Button>
                ) : null
            ],
            [
              "id",
              "Details",
              ({ value }) => (
                <Button
                  size="compact-sm"
                  variant={runDetail?.run?.id === value ? "light" : "subtle"}
                  rightSection={<IconArrowRight size={14} />}
                  loading={runDetailLoading && runDetail?.run?.id === value}
                  onClick={() => onSelectRun(value)}
                >
                  Details
                </Button>
              )
            ]
          ]}
        />
      </DataPanel>

      <DataPanel title="Run detail">
        <RunDetail
          detail={runDetail}
          loading={runDetailLoading}
          onRetryRun={onRetryRun}
          retryingRunId={retryingRunId}
          onOpenService={onOpenService}
        />
      </DataPanel>
    </Stack>
  );
}

function RunDetail({ detail, loading, onRetryRun, retryingRunId, onOpenService }) {
  if (loading && !detail) {
    return (
      <Box className="runDetailLoader">
        <Loader size="sm" />
      </Box>
    );
  }

  if (!detail) {
    return <EmptyText>Select a run to inspect imported records and item errors</EmptyText>;
  }

  const run = detail.run;
  const runItems = detail.items || [];
  const healthChecks = detail.health_checks || [];
  const itemErrors = detail.item_errors || [];

  return (
    <Stack gap="lg" className="runDetail">
      <Group justify="space-between" align="flex-start" gap="lg" className="runDetailHeader">
        <Box className="runDetailIdentity">
          <Text fw={850} size="lg">
            Run #{run.id}
          </Text>
          <Text size="sm" c="dimmed" className="runDetailMeta">
            {run.source} - {run.target} - {run.trigger}
          </Text>
        </Box>
        <Group gap="xs">
          {canRetry(run) && (
            <Button
              size="compact-sm"
              variant="light"
              leftSection={<IconRefresh size={14} />}
              loading={retryingRunId === run.id}
              onClick={() => onRetryRun(run)}
            >
              Retry
            </Button>
          )}
          <StatusBadge value={run.status} />
        </Group>
      </Group>

      <SimpleGrid cols={{ base: 2, sm: 4 }} className="runDetailMetrics">
        <RunMetric label="Imported" value={run.success_count} />
        <RunMetric label="Failed" value={run.failure_count} tone={run.failure_count > 0 ? "failed" : ""} />
        <RunMetric label="Run items" value={runItems.length} />
        <RunMetric label="Duration" value={`${run.duration_ms} ms`} />
      </SimpleGrid>

      {run.error_message && (
        <Box className="runDetailError">
          <Text size="xs" c="dimmed" fw={700} tt="uppercase">
            Error
          </Text>
          <Text size="sm">{run.error_message}</Text>
        </Box>
      )}

      <Box>
        <SectionTitle title="Run items" count={runItems.length} />
        <DataTable
          rows={runItems}
          columns={[
            [
              "record_id",
              "Record",
              ({ value, row }) =>
                value && row.target === "service_health" && onOpenService ? (
                  <Button
                    size="compact-sm"
                    variant="subtle"
                    rightSection={<IconArrowRight size={14} />}
                    onClick={() => onOpenService(value)}
                  >
                    #{value}
                  </Button>
                ) : (
                  value ? `#${value}` : ""
                )
            ],
            ["target", "Target"],
            ["external_id", "External ID"],
            ["status", "Status", StatusBadge],
            ["snapshot", "Snapshot", SnapshotCell],
            ["created_at", "Created", DateCell]
          ]}
        />
      </Box>

      <Box>
        <SectionTitle title="Service health checks" count={healthChecks.length} />
        <DataTable
          rows={healthChecks}
          columns={[
            [
              "service_id",
              "Service",
              ({ value }) =>
                onOpenService ? (
                  <Button
                    size="compact-sm"
                    variant="subtle"
                    rightSection={<IconArrowRight size={14} />}
                    onClick={() => onOpenService(value)}
                  >
                    #{value}
                  </Button>
                ) : (
                  value
                )
            ],
            ["external_id", "External ID"],
            ["health_status", "Health", StatusBadge],
            ["previous_health_status", "Previous", StatusBadge],
            ["checked_at", "Checked", DateCell]
          ]}
        />
      </Box>

      <Box>
        <SectionTitle title="Item errors" count={itemErrors.length} />
        <DataTable
          rows={itemErrors}
          columns={[
            ["external_id", "External ID"],
            ["message", "Message"],
            ["raw_item", "Raw item", SnapshotCell],
            ["created_at", "Created", DateCell]
          ]}
        />
      </Box>
    </Stack>
  );
}

function canRetry(run?: { status?: string }) {
  return ["failed", "partial_success"].includes(run?.status);
}

function RunMetric({
  label,
  value,
  tone = ""
}: {
  label: string;
  value: ReactNode;
  tone?: string;
}) {
  return (
    <Box className={`runDetailMetric${tone ? ` is-${tone}` : ""}`}>
      <Text size="xs" c="dimmed" fw={700} tt="uppercase">
        {label}
      </Text>
      <Text fw={850} className="runDetailMetricValue">
        {value ?? 0}
      </Text>
    </Box>
  );
}

function SectionTitle({ title, count }: { title: string; count: number }) {
  return (
    <Group justify="space-between" mb="xs" className="runDetailSectionTitle">
      <Text fw={800}>{title}</Text>
      <Text size="sm" c="dimmed">
        {count}
      </Text>
    </Group>
  );
}

function SnapshotCell({ value }: { value?: unknown }) {
  const [opened, setOpened] = useState(false);

  if (!value) {
    return <Text c="dimmed">None</Text>;
  }

  return (
    <>
      <Button
        size="compact-sm"
        variant="subtle"
        leftSection={<IconEye size={14} />}
        onClick={() => setOpened(true)}
      >
        View
      </Button>
      <Modal opened={opened} onClose={() => setOpened(false)} title="Sanitized snapshot" size="xl" centered>
        <Code block className="metadataRawJson">
          {prettyJson(value)}
        </Code>
      </Modal>
    </>
  );
}
