import { Box, Grid, Group, Loader, Paper, SimpleGrid, Skeleton, Stack } from "@mantine/core";

import { DataPanel } from "./DataPanel.jsx";

export function PageLoader() {
  return (
    <Paper p="lg" withBorder className="pageLoader">
      <Loader size="sm" />
    </Paper>
  );
}

export function DashboardSkeleton() {
  return (
    <Stack gap="lg">
      <SimpleGrid cols={{ base: 1, sm: 2, lg: 4 }}>
        {Array.from({ length: 4 }).map((_, index) => (
          <MetricSkeleton key={index} />
        ))}
      </SimpleGrid>
      <Grid>
        <Grid.Col span={{ base: 12, md: 6 }}>
          <PanelSkeleton title="My services" />
        </Grid.Col>
        <Grid.Col span={{ base: 12, md: 6 }}>
          <PanelSkeleton title="Open work" />
        </Grid.Col>
      </Grid>
    </Stack>
  );
}

export function CatalogSkeleton() {
  return (
    <Grid>
      <Grid.Col span={{ base: 12, md: 6 }}>
        <PanelSkeleton title="Services" />
      </Grid.Col>
      <Grid.Col span={{ base: 12, md: 6 }}>
        <PanelSkeleton title="Packages" />
      </Grid.Col>
    </Grid>
  );
}

export function AuditSkeleton() {
  return <PanelSkeleton title="Audit log" rows={6} />;
}

export function ConnectorsSkeleton() {
  return (
    <Grid>
      <Grid.Col span={{ base: 12, md: 4 }}>
        <PanelSkeleton title="Registry" rows={5} />
      </Grid.Col>
      <Grid.Col span={{ base: 12, md: 8 }}>
        <Stack gap="md">
          <PanelSkeleton title="Connector" rows={7} />
          <PanelSkeleton title="Recent runs" rows={5} />
        </Stack>
      </Grid.Col>
    </Grid>
  );
}

export function ServiceOverviewSkeleton() {
  return (
    <Stack gap="md">
      <PanelSkeleton title="Service" rows={4} />
      <Grid>
        <Grid.Col span={{ base: 12, md: 6 }}>
          <PanelSkeleton title="Packages" rows={4} />
        </Grid.Col>
        <Grid.Col span={{ base: 12, md: 6 }}>
          <PanelSkeleton title="Recent connector runs" rows={4} />
        </Grid.Col>
      </Grid>
    </Stack>
  );
}

function MetricSkeleton() {
  return (
    <Paper p={{ base: "sm", sm: "md" }} withBorder>
      <Stack gap="xs">
        <Skeleton height={14} width="42%" radius="sm" />
        <Skeleton height={30} width="62%" radius="sm" />
      </Stack>
    </Paper>
  );
}

function PanelSkeleton({ title, rows = 5 }) {
  return (
    <DataPanel title={title}>
      <Stack gap="sm">
        {Array.from({ length: rows }).map((_, index) => (
          <Group key={index} gap="sm" wrap="nowrap">
            <Skeleton height={16} width={index % 2 === 0 ? "42%" : "32%"} radius="sm" />
            <Box flex={1}>
              <Skeleton height={16} radius="sm" />
            </Box>
          </Group>
        ))}
      </Stack>
    </DataPanel>
  );
}
