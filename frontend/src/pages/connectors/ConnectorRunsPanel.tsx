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
import { IconArrowRight, IconEye, IconRefresh, IconX } from "@tabler/icons-react";
import { useState } from "react";
import type { ReactNode } from "react";

import { DataPanel } from "../../components/DataPanel";
import { DataTable } from "../../components/DataTable";
import { EmptyText } from "../../components/EmptyText";
import { DateCell, StatusBadge } from "../../components/tableCells";
import { prettyJson } from "../../utils/format";
import type { ApiId, ConnectorRun, ConnectorRunDetail } from "../../types/api";

type RunDetailOptions = {
  source?: string;
};

export function ConnectorRunsPanel({
  runs,
  runDetail,
  runDetailLoading,
  onSelectRun,
  onRetryRun,
  retryingRunId,
  onCancelRun,
  cancellingRunId,
  onOpenService
}: {
  runs: ConnectorRun[];
  runDetail: ConnectorRunDetail | null;
  runDetailLoading: boolean;
  onSelectRun: (runId: string | number, options?: RunDetailOptions) => void | Promise<void>;
  onRetryRun: (run: ConnectorRun) => void | Promise<void>;
  retryingRunId: ApiId | string | null;
  onCancelRun: (run: ConnectorRun) => void | Promise<void>;
  cancellingRunId: ApiId | string | null;
  onOpenService: (serviceId: string | number) => void;
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
            ["archived_count", "Archived"],
            ["duration_ms", "MS"],
            ["started_at", "Started", DateCell],
            [
              "id",
              "Cancel",
              ({ row }) =>
                canCancel(row) ? (
                  <Button
                    size="compact-sm"
                    variant="subtle"
                    color="red"
                    aria-label={`Cancel run #${row.id}`}
                    leftSection={<IconX size={14} />}
                    loading={cancellingRunId === row.id}
                    onClick={() => onCancelRun(row)}
                  >
                    Cancel
                  </Button>
                ) : null
            ],
            [
              "id",
              "Retry",
              ({ row }) =>
                canRetry(row) ? (
                  <Button
                    size="compact-sm"
                    variant="subtle"
                    aria-label={`Retry run #${row.id}`}
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
              ({ value }) => {
                const runId = Number(value);

                return (
                  <Button
                    size="compact-sm"
                    variant={runDetail?.run?.id === runId ? "light" : "subtle"}
                    aria-label={`Inspect run #${runId}`}
                    rightSection={<IconArrowRight size={14} />}
                    loading={runDetailLoading && runDetail?.run?.id === runId}
                    onClick={() => onSelectRun(runId)}
                  >
                    Details
                  </Button>
                );
              }
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
          onCancelRun={onCancelRun}
          cancellingRunId={cancellingRunId}
          onOpenService={onOpenService}
        />
      </DataPanel>
    </Stack>
  );
}

function RunDetail({
  detail,
  loading,
  onRetryRun,
  retryingRunId,
  onCancelRun,
  cancellingRunId,
  onOpenService
}: {
  detail: ConnectorRunDetail | null;
  loading: boolean;
  onRetryRun: (run: ConnectorRun) => void | Promise<void>;
  retryingRunId: ApiId | string | null;
  onCancelRun: (run: ConnectorRun) => void | Promise<void>;
  cancellingRunId: ApiId | string | null;
  onOpenService: (serviceId: string | number) => void;
}) {
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
          {canCancel(run) && (
            <Button
              size="compact-sm"
              variant="light"
              color="red"
              leftSection={<IconX size={14} />}
              loading={cancellingRunId === run.id}
              onClick={() => onCancelRun(run)}
            >
              Cancel
            </Button>
          )}
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
        <RunMetric label="Imported" value={run.success_count} tone="imported" />
        <RunMetric
          label="Failed"
          value={run.failure_count}
          tone={run.failure_count > 0 ? "failed" : "clean"}
        />
        <RunMetric label="Run items" value={runItems.length} tone="items" />
        <RunMetric label="Duration" value={`${run.duration_ms} ms`} tone="duration" />
      </SimpleGrid>

      <SimpleGrid cols={{ base: 2, sm: 3 }} className="runDetailMetrics">
        <RunMetric label="Attempt" value={`${run.attempt_count} / ${run.max_attempts}`} />
        <RunMetric label="Snapshot" value={formatSnapshotState(run)} />
        <RunMetric label="Archived" value={run.archived_count} />
        <RunMetric label="Next attempt" value={formatRunDate(run.next_attempt_at)} />
        <RunMetric label="Lease heartbeat" value={formatRunDate(run.heartbeat_at)} />
        <RunMetric
          label="Cancellation"
          value={run.cancelled_at ? "Cancelled" : run.cancel_requested_at ? "Requested" : "-"}
        />
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
              ({ value, row }) => {
                const recordId = value ? Number(value) : null;

                return recordId && row.target === "service_health" && onOpenService ? (
                  <Button
                    size="compact-sm"
                    variant="subtle"
                    aria-label={`Open service #${recordId} from run item`}
                    rightSection={<IconArrowRight size={14} />}
                    onClick={() => onOpenService(recordId)}
                  >
                    #{recordId}
                  </Button>
                ) : (
                  recordId ? `#${recordId}` : ""
                );
              }
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
              ({ value }) => {
                const serviceId = Number(value);

                return onOpenService ? (
                  <Button
                    size="compact-sm"
                    variant="subtle"
                    aria-label={`Open service #${serviceId} from health check`}
                    rightSection={<IconArrowRight size={14} />}
                    onClick={() => onOpenService(serviceId)}
                  >
                    #{serviceId}
                  </Button>
                ) : (
                  String(value)
                );
              }
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

function canRetry(run?: Pick<ConnectorRun, "status"> | null) {
  return Boolean(run?.status && ["failed", "partial_success", "cancelled"].includes(run.status));
}

function canCancel(run?: Pick<ConnectorRun, "status"> | null) {
  return Boolean(run?.status && ["queued", "running"].includes(run.status));
}

function formatRunDate(value?: string | null): string {
  return value ? new Date(value).toLocaleString() : "-";
}

function formatSnapshotState(run: ConnectorRun): string {
  if (run.snapshot_complete === null) {
    return "Not declared";
  }
  if (!run.snapshot_complete) {
    return "Incomplete - existing records kept";
  }
  if (run.failure_count > 0) {
    return "Complete - item errors, no archive";
  }
  return "Complete";
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
