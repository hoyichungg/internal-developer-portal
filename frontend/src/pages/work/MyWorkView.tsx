import {
  Box,
  Button,
  Grid,
  Group,
  Pagination,
  Paper,
  Select,
  Stack,
  Text,
  Title
} from "@mantine/core";
import { IconExternalLink, IconRefresh, IconX } from "@tabler/icons-react";
import { useEffect, useMemo } from "react";

import type { ApiClient } from "../../api/client";
import { DataPanel } from "../../components/DataPanel";
import { EmptyText } from "../../components/EmptyText";
import { PageLoader } from "../../components/LoadingState";
import { StatusBadge } from "../../components/tableCells";
import { ViewFrame } from "../../components/ViewFrame";
import { useAsyncData } from "../../hooks/useAsyncData";
import { useRefresh } from "../../hooks/useRefresh";
import type { ApiId, DateTimeString, MyWorkResponse, WorkCard } from "../../types/api";
import {
  DEFAULT_MY_WORK_QUERY,
  MY_WORK_PAGE_SIZES,
  myWorkApiPath,
  myWorkDetailHash,
  myWorkHash,
  parseMyWorkQuery
} from "./myWorkRouting";
import type { MyWorkQuery } from "./myWorkRouting";

const DUE_OPTIONS = [
  { value: "overdue", label: "Overdue" },
  { value: "today", label: "Due today (UTC)" },
  { value: "next_7_days", label: "Next 7 days" },
  { value: "none", label: "No due date" }
];

const SORT_OPTIONS = [
  { value: "attention", label: "Needs attention" },
  { value: "due_asc", label: "Due date (soonest)" },
  { value: "source_updated_desc", label: "Source updated" }
];

export function MyWorkView({
  client,
  searchParams,
  onNavigate,
  onOpenWorkCard
}: {
  client: ApiClient;
  searchParams: URLSearchParams;
  onNavigate: (hash: string) => void;
  onOpenWorkCard: (workCardId: ApiId, detailHash: string) => void;
}) {
  const rawSearch = searchParams.toString();
  const query = useMemo(() => parseMyWorkQuery(rawSearch), [rawSearch]);
  const canonicalHash = myWorkHash(query);
  const apiPath = myWorkApiPath(query);
  const [data, actions] = useAsyncData<MyWorkResponse>(
    () => client.get<MyWorkResponse>(apiPath),
    [client, apiPath]
  );

  useRefresh(actions.reload);

  useEffect(() => {
    const currentRoute = window.location.hash.replace(/^#\/?/, "").split("?", 1)[0];
    if (currentRoute !== "my-work" || window.location.hash === canonicalHash) {
      return;
    }

    window.history.replaceState(
      window.history.state,
      "",
      `${window.location.pathname}${window.location.search}${canonicalHash}`
    );
  }, [canonicalHash]);

  const response = data.value;
  const totalPages = Math.max(1, Math.ceil((response?.total || 0) / query.pageSize));

  useEffect(() => {
    if (!data.loading && response && query.page > totalPages) {
      onNavigate(myWorkHash({ ...query, page: totalPages }));
    }
  }, [data.loading, onNavigate, query, response, totalPages]);

  function updateQuery(patch: Partial<MyWorkQuery>, resetPage = true) {
    onNavigate(
      myWorkHash({
        ...query,
        ...patch,
        page: resetPage ? 1 : patch.page || query.page
      })
    );
  }

  function resetFilters() {
    onNavigate(myWorkHash({ ...DEFAULT_MY_WORK_QUERY, pageSize: query.pageSize }));
  }

  const activeFilterCount = [
    query.status,
    query.due,
    query.project,
    query.workItemType,
    query.source
  ].filter(Boolean).length;
  const facets = response?.facets || {
    statuses: [],
    projects: [],
    work_item_types: [],
    sources: []
  };

  return (
    <ViewFrame
      eyebrow="Personal workload"
      title="My Work"
      loading={data.loading && !response}
      loadingFallback={<PageLoader />}
      error={data.error}
      actions={
        <Button
          variant="default"
          size="sm"
          leftSection={<IconRefresh size={16} />}
          loading={data.loading}
          onClick={() => void actions.reload()}
        >
          Refresh
        </Button>
      }
    >
      {response && (
        <Stack gap="lg" className="myWorkPage">
          <DataPanel
            title="Filters"
            className="myWorkFilters"
            actions={
              <Button
                variant="subtle"
                size="compact-sm"
                leftSection={<IconX size={15} />}
                disabled={activeFilterCount === 0 && query.sort === DEFAULT_MY_WORK_QUERY.sort}
                onClick={resetFilters}
              >
                Reset
              </Button>
            }
          >
            <Grid gutter="sm">
              <Grid.Col span={{ base: 12, sm: 6, lg: 3 }}>
                <Select
                  label="Status"
                  placeholder="All statuses"
                  clearable
                  searchable
                  data={facetOptions(facets.statuses, query.status)}
                  value={query.status || null}
                  onChange={(value) => updateQuery({ status: value || "" })}
                />
              </Grid.Col>
              <Grid.Col span={{ base: 12, sm: 6, lg: 3 }}>
                <Select
                  label="Due"
                  placeholder="Any due date"
                  clearable
                  data={DUE_OPTIONS}
                  value={query.due || null}
                  onChange={(value) => updateQuery({ due: parseMyWorkQuery(`due=${value || ""}`).due })}
                />
              </Grid.Col>
              <Grid.Col span={{ base: 12, sm: 6, lg: 3 }}>
                <Select
                  label="Project"
                  placeholder="All projects"
                  clearable
                  searchable
                  data={facetOptions(facets.projects, query.project)}
                  value={query.project || null}
                  onChange={(value) => updateQuery({ project: value || "" })}
                />
              </Grid.Col>
              <Grid.Col span={{ base: 12, sm: 6, lg: 3 }}>
                <Select
                  label="Work item type"
                  placeholder="All types"
                  clearable
                  searchable
                  data={facetOptions(facets.work_item_types, query.workItemType)}
                  value={query.workItemType || null}
                  onChange={(value) => updateQuery({ workItemType: value || "" })}
                />
              </Grid.Col>
              <Grid.Col span={{ base: 12, sm: 6, lg: 3 }}>
                <Select
                  label="Source"
                  placeholder="All sources"
                  clearable
                  searchable
                  data={facetOptions(facets.sources, query.source)}
                  value={query.source || null}
                  onChange={(value) => updateQuery({ source: value || "" })}
                />
              </Grid.Col>
              <Grid.Col span={{ base: 12, sm: 6, lg: 3 }}>
                <Select
                  label="Sort"
                  data={SORT_OPTIONS}
                  value={query.sort}
                  allowDeselect={false}
                  onChange={(value) =>
                    updateQuery({ sort: parseMyWorkQuery(`sort=${value || ""}`).sort })
                  }
                />
              </Grid.Col>
              <Grid.Col span={{ base: 12, sm: 6, lg: 3 }}>
                <Select
                  label="Items per page"
                  data={MY_WORK_PAGE_SIZES.map((size) => ({
                    value: String(size),
                    label: String(size)
                  }))}
                  value={String(query.pageSize)}
                  allowDeselect={false}
                  onChange={(value) =>
                    updateQuery({ pageSize: parseMyWorkQuery(`page_size=${value || ""}`).pageSize })
                  }
                />
              </Grid.Col>
            </Grid>
            <Text size="sm" c="dimmed" mt="md">
              {activeFilterCount > 0
                ? `${activeFilterCount} active filter${activeFilterCount === 1 ? "" : "s"}`
                : "Showing all work assigned to you"}
            </Text>
          </DataPanel>

          <DataPanel
            title="Assigned to me"
            className="myWorkResults"
            actions={<ResultCount total={response.total} page={query.page} pageSize={query.pageSize} />}
          >
            {response.items.length === 0 ? (
              <EmptyText>
                {activeFilterCount > 0
                  ? "No assigned work matches these filters."
                  : "No work is currently assigned to you."}
              </EmptyText>
            ) : (
              <Stack gap="sm" className="myWorkList">
                {response.items.map((card) => (
                  <MyWorkCard
                    key={card.id}
                    card={card}
                    onOpen={() => onOpenWorkCard(card.id, myWorkDetailHash(card.id, query))}
                  />
                ))}
              </Stack>
            )}

            {response.total > query.pageSize && (
              <Group justify="space-between" align="center" mt="lg" className="myWorkPagination">
                <Text size="sm" c="dimmed">
                  Page {Math.min(query.page, totalPages)} of {totalPages}
                </Text>
                <Pagination
                  total={totalPages}
                  value={Math.min(query.page, totalPages)}
                  onChange={(page) => updateQuery({ page }, false)}
                  withEdges
                  aria-label="My Work pages"
                />
              </Group>
            )}
          </DataPanel>
        </Stack>
      )}
    </ViewFrame>
  );
}

function MyWorkCard({ card, onOpen }: { card: WorkCard; onOpen: () => void }) {
  const blocked = card.status.trim().toLowerCase() === "blocked";
  const overdue = isOverdue(card);
  const context = [card.project, card.work_item_type, card.source].filter(Boolean).join(" · ");

  return (
    <Paper
      p={{ base: "sm", sm: "md" }}
      withBorder
      className={`myWorkCard${blocked ? " isBlocked" : ""}${overdue ? " isOverdue" : ""}`}
    >
      <Group justify="space-between" align="flex-start" wrap="nowrap" className="myWorkCardLayout">
        <Box className="myWorkCardMain">
          <Group gap="xs" mb={6} wrap="wrap">
            <StatusBadge value={card.status} />
            <StatusBadge value={card.priority} />
            {overdue && <StatusBadge value="overdue" />}
          </Group>
          <Title order={3} size="h4" className="myWorkCardTitle">
            {card.title}
          </Title>
          <Text size="sm" c="dimmed" mt={4}>
            {context || "No project or source context"}
          </Text>
          <Group gap="lg" mt="sm" wrap="wrap" className="myWorkCardDates">
            <Text size="sm" fw={overdue ? 700 : 500} c={overdue ? "red" : undefined}>
              {card.due_at ? `Due ${formatDate(card.due_at)}` : "No due date"}
            </Text>
            <Text size="sm" c="dimmed">
              Source updated {formatDate(card.source_updated_at || card.updated_at)}
            </Text>
          </Group>
        </Box>

        <Stack gap="xs" align="flex-end" className="myWorkCardActions">
          <Button size="compact-sm" variant="light" onClick={onOpen}>
            Details
          </Button>
          {card.url && (
            <Button
              component="a"
              href={card.url}
              target="_blank"
              rel="noreferrer"
              size="compact-sm"
              variant="subtle"
              rightSection={<IconExternalLink size={14} />}
            >
              External card
            </Button>
          )}
        </Stack>
      </Group>
    </Paper>
  );
}

function ResultCount({ total, page, pageSize }: { total: number; page: number; pageSize: number }) {
  if (total === 0) {
    return <Text c="dimmed">0 items</Text>;
  }

  const first = (page - 1) * pageSize + 1;
  const last = Math.min(page * pageSize, total);
  return (
    <Text size="sm" c="dimmed">
      {first}-{last} of {total}
    </Text>
  );
}

function facetOptions(values: string[], current: string) {
  const unique = new Set(values.filter(Boolean));
  if (current) {
    unique.add(current);
  }

  return Array.from(unique).map((value) => ({ value, label: value }));
}

function isOverdue(card: WorkCard): boolean {
  if (!card.due_at || isCompleted(card.status)) {
    return false;
  }

  const due = Date.parse(card.due_at);
  return Number.isFinite(due) && due < Date.now();
}

function isCompleted(status: string): boolean {
  return ["closed", "completed", "done", "removed", "resolved"].includes(
    status.trim().toLowerCase()
  );
}

function formatDate(value?: DateTimeString | null): string {
  if (!value) {
    return "-";
  }

  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "-" : date.toLocaleString();
}
