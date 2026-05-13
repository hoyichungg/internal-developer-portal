import { Button, Grid, Group, Select, Stack, Text, TextInput } from "@mantine/core";
import { IconFilter, IconRefresh, IconX } from "@tabler/icons-react";
import { useMemo, useState } from "react";
import type { FormEvent } from "react";

import type { ApiClient } from "../../api/client";
import { DataPanel } from "../../components/DataPanel";
import { DataTable } from "../../components/DataTable";
import { AuditSkeleton } from "../../components/LoadingState";
import { DateCell, StatusBadge } from "../../components/tableCells";
import { ViewFrame } from "../../components/ViewFrame";
import { useAsyncData } from "../../hooks/useAsyncData";
import { useRefresh } from "../../hooks/useRefresh";
import type { ApiId, AuditLog, UserSummary } from "../../types/api";
import { AuditMetadataCell } from "./AuditMetadataCell";

const AUDIT_LOG_VISIBLE_ROWS = 30;

type AuditFilters = {
  resource_type: string;
  resource_id: string;
  actor_user_id: string;
  action: string;
  created_from: string;
  created_to: string;
};

const emptyAuditFilters: AuditFilters = {
  resource_type: "",
  resource_id: "",
  actor_user_id: "",
  action: "",
  created_from: "",
  created_to: ""
};

export function AuditView({ client }: { client: ApiClient }) {
  const [draftFilters, setDraftFilters] = useState<AuditFilters>(emptyAuditFilters);
  const [appliedFilters, setAppliedFilters] = useState<AuditFilters>(emptyAuditFilters);
  const queryString = useMemo(() => auditQueryString(appliedFilters), [appliedFilters]);
  const [data, actions] = useAsyncData<AuditLog[]>(
    () => client.get<AuditLog[]>(`/audit-logs${queryString}`),
    [client, queryString]
  );
  const [usersData] = useAsyncData<UserSummary[]>(
    () => client.get<UserSummary[]>("/users"),
    [client]
  );

  useRefresh(actions.reload);

  const auditLogs = data.value;
  const visibleAuditLogs = auditLogs?.slice(0, AUDIT_LOG_VISIBLE_ROWS) || [];
  const userById = useMemo(() => {
    const lookup = new Map<ApiId, UserSummary>();
    (usersData.value || []).forEach((user) => lookup.set(user.id, user));
    return lookup;
  }, [usersData.value]);
  const actorOptions = useMemo(
    () =>
      (usersData.value || []).map((user) => ({
        value: String(user.id),
        label: `${user.username} (#${user.id})`
      })),
    [usersData.value]
  );
  const activeFilterCount = Object.values(appliedFilters).filter(Boolean).length;

  function updateFilter(field: keyof AuditFilters, value: string) {
    setDraftFilters((current) => ({ ...current, [field]: value }));
  }

  function applyFilters(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setAppliedFilters(normalizeFilters(draftFilters));
  }

  function clearFilters() {
    setDraftFilters(emptyAuditFilters);
    setAppliedFilters(emptyAuditFilters);
  }

  const ActorCell = ({ value }: { value?: unknown }) => {
    if (!value) {
      return <Text size="sm" c="dimmed">system</Text>;
    }

    const user = userById.get(Number(value));

    return (
      <Stack gap={0}>
        <Text size="sm" fw={700}>
          {user?.username || `User ${String(value)}`}
        </Text>
        <Text size="xs" c="dimmed">
          #{String(value)}
        </Text>
      </Stack>
    );
  };

  return (
    <ViewFrame
      eyebrow="Controls"
      title="Audit"
      loading={data.loading && !data.value}
      loadingFallback={<AuditSkeleton />}
      error={data.error}
    >
      {auditLogs && (
        <Stack gap="lg">
          <DataPanel title="Filters" className="auditFilterPanel">
            <form onSubmit={applyFilters}>
              <Stack gap="md">
                <Grid>
                  <Grid.Col span={{ base: 12, sm: 6, lg: 2 }}>
                    <TextInput
                      label="Resource type"
                      value={draftFilters.resource_type}
                      onChange={(event) => updateFilter("resource_type", event.currentTarget.value)}
                    />
                  </Grid.Col>
                  <Grid.Col span={{ base: 12, sm: 6, lg: 2 }}>
                    <TextInput
                      label="Resource ID"
                      value={draftFilters.resource_id}
                      onChange={(event) => updateFilter("resource_id", event.currentTarget.value)}
                    />
                  </Grid.Col>
                  <Grid.Col span={{ base: 12, sm: 6, lg: 2 }}>
                    <Select
                      label="Actor"
                      data={actorOptions}
                      value={draftFilters.actor_user_id || null}
                      onChange={(value) => updateFilter("actor_user_id", value || "")}
                      searchable
                      clearable
                    />
                  </Grid.Col>
                  <Grid.Col span={{ base: 12, sm: 6, lg: 2 }}>
                    <TextInput
                      label="Action"
                      value={draftFilters.action}
                      onChange={(event) => updateFilter("action", event.currentTarget.value)}
                    />
                  </Grid.Col>
                  <Grid.Col span={{ base: 12, sm: 6, lg: 2 }}>
                    <TextInput
                      label="From"
                      type="date"
                      value={draftFilters.created_from}
                      onChange={(event) => updateFilter("created_from", event.currentTarget.value)}
                    />
                  </Grid.Col>
                  <Grid.Col span={{ base: 12, sm: 6, lg: 2 }}>
                    <TextInput
                      label="To"
                      type="date"
                      value={draftFilters.created_to}
                      onChange={(event) => updateFilter("created_to", event.currentTarget.value)}
                    />
                  </Grid.Col>
                </Grid>
                <Group justify="space-between" align="center">
                  <Text size="sm" c="dimmed">
                    {activeFilterCount > 0 ? `${activeFilterCount} active` : "No filters"}
                  </Text>
                  <Group gap="xs">
                    <Button
                      type="button"
                      variant="default"
                      leftSection={<IconX size={16} />}
                      onClick={clearFilters}
                    >
                      Clear
                    </Button>
                    <Button
                      type="button"
                      variant="light"
                      leftSection={<IconRefresh size={16} />}
                      onClick={actions.reload}
                    >
                      Refresh
                    </Button>
                    <Button type="submit" leftSection={<IconFilter size={16} />}>
                      Apply
                    </Button>
                  </Group>
                </Group>
              </Stack>
            </form>
          </DataPanel>

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
                  ["actor_user_id", "Actor", ActorCell],
                  ["action", "Action", StatusBadge],
                  ["resource_type", "Resource", AuditCompactCell],
                  ["resource_id", "ID", AuditCompactCell],
                  ["metadata", "Metadata", AuditMetadataCell]
                ]}
              />
            </div>
          </DataPanel>
        </Stack>
      )}
    </ViewFrame>
  );
}

function auditQueryString(filters: AuditFilters): string {
  const params = new URLSearchParams();

  Object.entries(normalizeFilters(filters)).forEach(([key, value]) => {
    if (value) {
      params.set(key, value);
    }
  });

  const query = params.toString();
  return query ? `?${query}` : "";
}

function normalizeFilters(filters: AuditFilters): AuditFilters {
  return {
    resource_type: filters.resource_type.trim(),
    resource_id: filters.resource_id.trim(),
    actor_user_id: filters.actor_user_id.trim(),
    action: filters.action.trim(),
    created_from: filters.created_from.trim(),
    created_to: filters.created_to.trim()
  };
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
