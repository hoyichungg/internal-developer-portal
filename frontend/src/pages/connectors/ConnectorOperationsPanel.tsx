import { Box, Grid, Group, SimpleGrid, Text } from "@mantine/core";

import { DataPanel } from "../../components/DataPanel";
import { DataTable } from "../../components/DataTable";
import { EmptyText } from "../../components/EmptyText";
import { DateCell, StatusBadge } from "../../components/tableCells";

export function ConnectorOperationsPanel({ operations }) {
  const workers = operations?.workers || [];
  const maintenanceRuns = operations?.maintenance_runs || [];
  const activeWorkers = workers.filter((worker) => !worker.is_stale).length;
  const staleWorkers = workers.filter((worker) => worker.is_stale).length;
  const latestCleanup = maintenanceRuns[0];

  return (
    <Grid>
      <Grid.Col span={{ base: 12, lg: 5 }}>
        <DataPanel title="Worker heartbeat">
          <SimpleGrid cols={{ base: 2, sm: 4 }} className="operationsMetrics">
            <OperationMetric label="Active" value={activeWorkers} tone="success" />
            <OperationMetric label="Stale" value={staleWorkers} tone={staleWorkers > 0 ? "warning" : "success"} />
            <OperationMetric label="Seen" value={workers.length} tone="info" />
            <OperationMetric label="Stale after" value={`${operations?.stale_after_seconds || 0}s`} tone="threshold" />
          </SimpleGrid>

          {workers.length > 0 ? (
            <DataTable
              rows={workers}
              columns={[
                ["worker_id", "Worker"],
                ["status", "Status", StatusBadge],
                ["scheduler_enabled", "Scheduler", BooleanCell],
                ["retention_enabled", "Retention", BooleanCell],
                ["current_run_id", "Run", RunIdCell],
                ["last_seen_at", "Last seen", DateCell]
              ]}
            />
          ) : (
            <EmptyText>No worker heartbeat yet</EmptyText>
          )}
        </DataPanel>
      </Grid.Col>

      <Grid.Col span={{ base: 12, lg: 7 }}>
        <DataPanel title="Retention cleanup history">
          <SimpleGrid cols={{ base: 2, sm: 4 }} className="operationsMetrics">
            <OperationMetric
              label="Last status"
              value={latestCleanup?.status || "none"}
              tone={latestCleanup?.status || ""}
              badge
            />
            <OperationMetric label="Health" value={latestCleanup?.health_checks_deleted || 0} tone="health" />
            <OperationMetric label="Runs" value={latestCleanup?.connector_runs_deleted || 0} tone="runs" />
            <OperationMetric label="Audit" value={latestCleanup?.audit_logs_deleted || 0} tone="audit" />
          </SimpleGrid>

          <DataTable
            rows={maintenanceRuns}
            columns={[
              ["status", "Status", StatusBadge],
              ["worker_id", "Worker"],
              ["health_checks_deleted", "Health"],
              ["connector_runs_deleted", "Runs"],
              ["audit_logs_deleted", "Audit"],
              ["duration_ms", "MS"],
              ["finished_at", "Finished", DateCell]
            ]}
          />
        </DataPanel>
      </Grid.Col>
    </Grid>
  );
}

function OperationMetric({ label, value, tone, badge = false }) {
  return (
    <Box className={`operationMetric${tone ? ` is-${tone}` : ""}`}>
      <Text size="xs" c="dimmed" fw={700} tt="uppercase">
        {label}
      </Text>
      {badge ? (
        <StatusBadge value={value} />
      ) : (
        <Text fw={850} className="operationMetricValue">
          {value ?? 0}
        </Text>
      )}
    </Box>
  );
}

function BooleanCell({ value }) {
  return <StatusBadge value={value ? "active" : "paused"} />;
}

function RunIdCell({ value }) {
  if (!value) {
    return <Text c="dimmed">None</Text>;
  }

  return (
    <Group gap={4} wrap="nowrap">
      <Text size="sm">#{value}</Text>
    </Group>
  );
}
