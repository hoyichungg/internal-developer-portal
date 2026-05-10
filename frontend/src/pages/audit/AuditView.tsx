import { Text } from "@mantine/core";

import type { ApiClient } from "../../api/client";
import { DataPanel } from "../../components/DataPanel";
import { DataTable } from "../../components/DataTable";
import { AuditSkeleton } from "../../components/LoadingState";
import { DateCell } from "../../components/tableCells";
import { ViewFrame } from "../../components/ViewFrame";
import { useAsyncData } from "../../hooks/useAsyncData";
import { useRefresh } from "../../hooks/useRefresh";
import type { AuditLog } from "../../types/api";
import { AuditMetadataCell } from "./AuditMetadataCell";

const AUDIT_LOG_VISIBLE_ROWS = 30;

export function AuditView({ client }: { client: ApiClient }) {
  const [data, actions] = useAsyncData<AuditLog[]>(
    () => client.get<AuditLog[]>("/audit-logs"),
    [client]
  );

  useRefresh(actions.reload);

  const auditLogs = data.value;
  const visibleAuditLogs = auditLogs?.slice(0, AUDIT_LOG_VISIBLE_ROWS) || [];

  return (
    <ViewFrame
      eyebrow="Controls"
      title="Audit"
      loading={data.loading && !data.value}
      loadingFallback={<AuditSkeleton />}
      error={data.error}
    >
      {auditLogs && (
        <DataPanel
          title="Audit log"
          className="auditLogPanel"
          actions={
            <Text size="sm" c="dimmed">
              Latest {visibleAuditLogs.length} of {auditLogs.length}
            </Text>
          }
        >
          <div className="auditLogTableFrame">
            <DataTable
              rows={visibleAuditLogs}
              columns={[
                ["created_at", "Created", DateCell],
                ["actor_user_id", "Actor"],
                ["action", "Action", AuditCompactCell],
                ["resource_type", "Resource", AuditCompactCell],
                ["resource_id", "ID", AuditCompactCell],
                ["metadata", "Metadata", AuditMetadataCell]
              ]}
            />
          </div>
        </DataPanel>
      )}
    </ViewFrame>
  );
}

function AuditCompactCell({ value }: { value?: unknown }) {
  if (!value) {
    return null;
  }

  return (
    <Text size="sm" className="auditCompactCell" title={String(value)}>
      {String(value)}
    </Text>
  );
}
