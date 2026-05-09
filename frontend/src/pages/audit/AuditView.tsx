import { DataPanel } from "../../components/DataPanel";
import { DataTable } from "../../components/DataTable";
import { AuditSkeleton } from "../../components/LoadingState";
import { DateCell } from "../../components/tableCells";
import { ViewFrame } from "../../components/ViewFrame";
import { useAsyncData } from "../../hooks/useAsyncData";
import { useRefresh } from "../../hooks/useRefresh";
import { AuditMetadataCell } from "./AuditMetadataCell";

export function AuditView({ client }) {
  const [data, actions] = useAsyncData(() => client.get("/audit-logs"), [client]);

  useRefresh(actions.reload);

  return (
    <ViewFrame
      eyebrow="Controls"
      title="Audit"
      loading={data.loading && !data.value}
      loadingFallback={<AuditSkeleton />}
      error={data.error}
    >
      {data.value && (
        <DataPanel title="Audit log">
          <DataTable
            rows={data.value}
            columns={[
              ["created_at", "Created", DateCell],
              ["actor_user_id", "Actor"],
              ["action", "Action"],
              ["resource_type", "Resource"],
              ["resource_id", "ID"],
              ["metadata", "Metadata", AuditMetadataCell]
            ]}
          />
        </DataPanel>
      )}
    </ViewFrame>
  );
}
