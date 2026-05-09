import { DataPanel } from "../../components/DataPanel.jsx";
import { DataTable } from "../../components/DataTable.jsx";
import { AuditSkeleton } from "../../components/LoadingState.jsx";
import { DateCell } from "../../components/tableCells.jsx";
import { ViewFrame } from "../../components/ViewFrame.jsx";
import { useAsyncData } from "../../hooks/useAsyncData.js";
import { useRefresh } from "../../hooks/useRefresh.js";
import { AuditMetadataCell } from "./AuditMetadataCell.jsx";

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
