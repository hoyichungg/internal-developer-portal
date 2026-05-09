import { Grid } from "@mantine/core";

import { DataPanel } from "../../components/DataPanel.jsx";
import { DataTable } from "../../components/DataTable.jsx";
import { CatalogSkeleton } from "../../components/LoadingState.jsx";
import { LinkCell, StatusBadge } from "../../components/tableCells.jsx";
import { ViewFrame } from "../../components/ViewFrame.jsx";
import { useAsyncData } from "../../hooks/useAsyncData.js";
import { useRefresh } from "../../hooks/useRefresh.js";

export function CatalogView({ client }) {
  const [data, actions] = useAsyncData(async () => {
    const [services, packages] = await Promise.all([
      client.get("/services"),
      client.get("/packages")
    ]);
    return { services, packages };
  }, [client]);

  useRefresh(actions.reload);

  return (
    <ViewFrame
      eyebrow="Ownership"
      title="Catalog"
      loading={data.loading && !data.value}
      loadingFallback={<CatalogSkeleton />}
      error={data.error}
    >
      {data.value && (
        <Grid>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <DataPanel title="Services">
              <DataTable
                rows={data.value.services}
                columns={[
                  ["name", "Service"],
                  ["health_status", "Health", StatusBadge],
                  ["lifecycle_status", "Lifecycle", StatusBadge],
                  ["source", "Source"]
                ]}
              />
            </DataPanel>
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <DataPanel title="Packages">
              <DataTable
                rows={data.value.packages}
                columns={[
                  ["name", "Package"],
                  ["version", "Version"],
                  ["status", "Status", StatusBadge],
                  ["repository_url", "Repo", LinkCell]
                ]}
              />
            </DataPanel>
          </Grid.Col>
        </Grid>
      )}
    </ViewFrame>
  );
}
