import { Button, Grid, Modal, Stack } from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import { IconPlus } from "@tabler/icons-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";

import { isApiError } from "../../api/client";
import type { ApiClient } from "../../api/client";
import type {
  ApiId,
  Connector,
  ConnectorConfigForm,
  ConnectorConfigResponse,
  ConnectorDrillTarget,
  MicrosoftOAuthAuthorizeResponse,
  ConnectorOperationsResponse,
  ConnectorRun,
  ConnectorRunDetail,
  ConnectorRunExecutionResponse,
  ConnectorScopePayload,
  Maintainer,
  NewConnectorPayload,
  UserSummary
} from "../../types/api";
import { ViewFrame } from "../../components/ViewFrame";
import { ConnectorsSkeleton } from "../../components/LoadingState";
import { useRefresh } from "../../hooks/useRefresh";
import { showError } from "../../utils/notifications";
import { ConnectorConfigEditor } from "./ConnectorConfigEditor";
import { ConnectorCreateForm } from "./ConnectorCreateForm";
import { ConnectorOperationsPanel } from "./ConnectorOperationsPanel";
import { ConnectorRegistry } from "./ConnectorRegistry";
import { ConnectorRunsPanel } from "./ConnectorRunsPanel";
import { ConnectorScopeForm } from "./ConnectorScopeForm";
import {
  connectorConfigFromResponse,
  connectorConfigFromTemplate,
  defaultConnectorConfig
} from "./connectorConfig";
import type { ConnectorConfigLoadState } from "./connectorConfig";

type ConnectorViewOptions = {
  preserveRunDetail?: boolean;
};

type RunDetailOptions = {
  source?: string;
};

type ReloadOptions = {
  runId?: ApiId | string | null;
};

function connectorDrillKey(target?: ConnectorDrillTarget | null): string {
  if (!target) {
    return "";
  }

  return [target.source || "", target.target || "", target.runId || ""].join("|");
}

function hasConnectorDrillTarget(target?: ConnectorDrillTarget | null): boolean {
  return Boolean(target?.source || target?.target || target?.runId);
}

export function ConnectorsView({
  client,
  drillTarget,
  onOpenService
}: {
  client: ApiClient;
  drillTarget?: ConnectorDrillTarget | null;
  onOpenService: (serviceId: string | number) => void;
}) {
  const [connectors, setConnectors] = useState<Connector[]>([]);
  const [operations, setOperations] = useState<ConnectorOperationsResponse | null>(null);
  const [selectedSource, setSelectedSource] = useState("");
  const [config, setConfig] = useState<ConnectorConfigForm>(defaultConnectorConfig);
  const [configLoadState, setConfigLoadState] = useState<ConnectorConfigLoadState>("idle");
  const [configLoadError, setConfigLoadError] = useState<Error | null>(null);
  const [runs, setRuns] = useState<ConnectorRun[]>([]);
  const [runDetail, setRunDetail] = useState<ConnectorRunDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [saving, setSaving] = useState(false);
  const [runLoading, setRunLoading] = useState(false);
  const [oauthLoading, setOauthLoading] = useState(false);
  const [retryingRunId, setRetryingRunId] = useState<string | number | null>(null);
  const [cancellingRunId, setCancellingRunId] = useState<string | number | null>(null);
  const [runDetailLoading, setRunDetailLoading] = useState(false);
  const [createOpened, createModal] = useDisclosure(false);
  const [scopeOpened, scopeModal] = useDisclosure(false);
  const [scopeSaving, setScopeSaving] = useState(false);
  const [maintainers, setMaintainers] = useState<Maintainer[]>([]);
  const [users, setUsers] = useState<UserSummary[]>([]);
  const [scopeOptionsLoading, setScopeOptionsLoading] = useState(false);
  const [scopeOptionsError, setScopeOptionsError] = useState<string | null>(null);
  const selectedSourceRef = useRef("");
  const detailsRequestSeqRef = useRef(0);
  const reloadRequestSeqRef = useRef(0);
  const runDetailRequestSeqRef = useRef(0);
  const refreshTimeoutRef = useRef<number | null>(null);
  const initialLoadStartedRef = useRef(false);
  const appliedDrillTargetKeyRef = useRef("");
  const selected = connectors.find((connector) => connector.source === selectedSource);
  const initialLoading = loading && connectors.length === 0;
  const configEditable = configLoadState === "ready" || configLoadState === "missing";

  useEffect(() => {
    selectedSourceRef.current = selectedSource;
  }, [selectedSource]);

  useEffect(() => () => {
    if (refreshTimeoutRef.current) {
      window.clearTimeout(refreshTimeoutRef.current);
    }
  }, []);

  const loadConnectorDetails = useCallback(
    async (source: string, options: ConnectorViewOptions = {}) => {
      const requestSeq = ++detailsRequestSeqRef.current;
      setConfigLoadState("loading");
      setConfigLoadError(null);
      if (!options.preserveRunDetail) {
        runDetailRequestSeqRef.current += 1;
        setRunDetail(null);
      }

      const [configResult, runsResult] = await Promise.allSettled([
        client
          .get<ConnectorConfigResponse>(`/connectors/${encodeURIComponent(source)}/config`)
          .catch((error: unknown) => {
            if (isApiError(error) && error.status === 404) {
              return null;
            }
            throw error;
          }),
        client.get<ConnectorRun[]>(`/connectors/runs?source=${encodeURIComponent(source)}`)
      ]);

      if (requestSeq !== detailsRequestSeqRef.current) {
        return;
      }

      if (configResult.status === "fulfilled") {
        setConfig(connectorConfigFromResponse(configResult.value));
        setConfigLoadState(configResult.value ? "ready" : "missing");
      } else {
        const error = toError(configResult.reason);
        setConfigLoadError(error);
        setConfigLoadState("error");
        showError(error);
      }

      if (runsResult.status === "fulfilled") {
        setRuns(runsResult.value);
      } else {
        showError(runsResult.reason);
      }
    },
    [client]
  );

  const loadOperations = useCallback(async () => {
    try {
      const nextOperations = await client.get<ConnectorOperationsResponse>("/connectors/operations");
      setOperations(nextOperations);
    } catch (error) {
      showError(error);
    }
  }, [client]);

  const loadRunDetail = useCallback(
    async (runId: string | number, options: RunDetailOptions = {}) => {
      const requestSeq = ++runDetailRequestSeqRef.current;
      setRunDetailLoading(true);
      try {
        const detail = await client.get<ConnectorRunDetail>(
          `/connectors/runs/${encodeURIComponent(runId)}`
        );
        const expectedSource = options.source || selectedSourceRef.current;
        if (requestSeq !== runDetailRequestSeqRef.current) {
          return;
        }
        if (expectedSource && detail.run?.source !== expectedSource) {
          return;
        }
        setRunDetail(detail);
      } catch (error) {
        if (requestSeq === runDetailRequestSeqRef.current) {
          showError(error);
        }
      } finally {
        if (requestSeq === runDetailRequestSeqRef.current) {
          setRunDetailLoading(false);
        }
      }
    },
    [client]
  );

  const reload = useCallback(async (preferredSource?: string, options: ReloadOptions = {}) => {
    const requestSeq = ++reloadRequestSeqRef.current;
    setLoading(true);
    try {
      const [nextConnectors, nextOperations] = await Promise.all([
        client.get<Connector[]>("/connectors"),
        client.get<ConnectorOperationsResponse>("/connectors/operations")
      ]);
      if (requestSeq !== reloadRequestSeqRef.current) {
        return;
      }
      setConnectors(nextConnectors);
      setOperations(nextOperations);
      const desiredSource = preferredSource || selectedSourceRef.current;
      const nextSource =
        desiredSource && nextConnectors.some((item) => item.source === desiredSource)
          ? desiredSource
          : nextConnectors[0]?.source || "";
      selectedSourceRef.current = nextSource;
      setSelectedSource(nextSource);
      if (nextSource) {
        await loadConnectorDetails(nextSource, { preserveRunDetail: Boolean(options.runId) });
        if (options.runId) {
          await loadRunDetail(options.runId, { source: nextSource });
        }
      } else {
        detailsRequestSeqRef.current += 1;
        runDetailRequestSeqRef.current += 1;
        setConfig(defaultConnectorConfig);
        setConfigLoadState("idle");
        setConfigLoadError(null);
        setRuns([]);
        setRunDetail(null);
      }
    } catch (error) {
      if (requestSeq === reloadRequestSeqRef.current) {
        showError(error);
      }
    } finally {
      if (requestSeq === reloadRequestSeqRef.current) {
        setLoading(false);
      }
    }
  }, [client, loadConnectorDetails, loadRunDetail]);

  useEffect(() => {
    const drillKey = connectorDrillKey(drillTarget);
    initialLoadStartedRef.current = true;
    appliedDrillTargetKeyRef.current = drillKey;
    reload(drillTarget?.source || undefined, { runId: drillTarget?.runId });
  }, []);

  useEffect(() => {
    if (!createOpened && !scopeOpened) {
      return;
    }

    let active = true;
    setScopeOptionsLoading(true);
    setScopeOptionsError(null);
    Promise.all([client.get<Maintainer[]>("/maintainers"), client.get<UserSummary[]>("/users")])
      .then(([nextMaintainers, nextUsers]) => {
        if (!active) return;
        setMaintainers(nextMaintainers);
        setUsers(nextUsers);
      })
      .catch((error: unknown) => {
        if (!active) return;
        setScopeOptionsError(toError(error).message);
      })
      .finally(() => {
        if (active) setScopeOptionsLoading(false);
      });

    return () => {
      active = false;
    };
  }, [client, createOpened, scopeOpened]);

  useEffect(() => {
    if (!initialLoadStartedRef.current || !hasConnectorDrillTarget(drillTarget)) {
      return;
    }

    const drillKey = connectorDrillKey(drillTarget);
    if (appliedDrillTargetKeyRef.current === drillKey) {
      return;
    }

    appliedDrillTargetKeyRef.current = drillKey;
    reload(drillTarget?.source || undefined, { runId: drillTarget?.runId });
  }, [drillTarget, reload]);

  useRefresh(() => reload());

  async function selectConnector(source: string) {
    if (refreshTimeoutRef.current) {
      window.clearTimeout(refreshTimeoutRef.current);
      refreshTimeoutRef.current = null;
    }
    selectedSourceRef.current = source;
    setSelectedSource(source);
    await loadConnectorDetails(source);
  }

  function scheduleRunRefresh(runSource: string, runId: string | number) {
    if (refreshTimeoutRef.current) {
      window.clearTimeout(refreshTimeoutRef.current);
    }
    refreshTimeoutRef.current = window.setTimeout(async () => {
      if (selectedSourceRef.current !== runSource) {
        return;
      }
      await loadConnectorDetails(runSource, { preserveRunDetail: true });
      await loadOperations();
      if (selectedSourceRef.current !== runSource) {
        return;
      }
      await loadRunDetail(runId, { source: runSource });
    }, 1200);
  }

  async function createConnector(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    setCreating(true);
    try {
      const payload: NewConnectorPayload = {
        source: String(form.get("source") || ""),
        kind: String(form.get("kind") || ""),
        display_name: String(form.get("display_name") || ""),
        status: String(form.get("status") || "active"),
        scope_type: String(form.get("scope_type")) as NewConnectorPayload["scope_type"],
        owner_user_id:
          form.get("scope_type") === "user" ? Number(form.get("owner_user_id")) : null,
        maintainer_id:
          form.get("scope_type") === "maintainer" ? Number(form.get("maintainer_id")) : null
      };
      const connector = await client.post<Connector>("/connectors", payload);
      event.currentTarget.reset();
      createModal.close();
      await reload(connector.source);
      notifications.show({ title: "Connector created", message: connector.source, color: "teal" });
    } catch (error) {
      showError(error);
    } finally {
      setCreating(false);
    }
  }

  async function saveConfig(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selected || !configEditable) {
      return;
    }
    setSaving(true);
    try {
      JSON.parse(config.config);
      JSON.parse(config.sample_payload);
      await client.put<ConnectorConfigResponse>(`/connectors/${encodeURIComponent(selected.source)}/config`, {
        target: config.target,
        enabled: config.enabled,
        schedule_cron: config.schedule_cron || null,
        config: config.config,
        sample_payload: config.sample_payload
      });
      await loadConnectorDetails(selected.source);
      await loadOperations();
      notifications.show({ title: "Config saved", message: selected.source, color: "teal" });
    } catch (error) {
      showError(error);
    } finally {
      setSaving(false);
    }
  }

  async function updateConnectorScope(payload: ConnectorScopePayload) {
    if (!selected) return;

    setScopeSaving(true);
    try {
      const connector = await client.put<Connector>(
        `/connectors/${encodeURIComponent(selected.source)}/scope`,
        payload
      );
      setConnectors((current) =>
        current.map((item) => (item.source === connector.source ? connector : item))
      );
      scopeModal.close();
      notifications.show({
        title: "Visibility updated",
        message: connector.display_name,
        color: "teal"
      });
    } catch (error) {
      showError(error);
    } finally {
      setScopeSaving(false);
    }
  }

  async function saveSelectedConfig(selectedConnector: Connector) {
    JSON.parse(config.config);
    JSON.parse(config.sample_payload);
    await client.put<ConnectorConfigResponse>(
      `/connectors/${encodeURIComponent(selectedConnector.source)}/config`,
      {
        target: config.target,
        enabled: config.enabled,
        schedule_cron: config.schedule_cron || null,
        config: config.config,
        sample_payload: config.sample_payload
      }
    );
  }

  async function connectMicrosoft() {
    if (!selected || !configEditable) {
      return;
    }
    const source = selected.source;
    setOauthLoading(true);
    try {
      await saveSelectedConfig(selected);
      const response = await client.post<MicrosoftOAuthAuthorizeResponse>(
        `/connectors/${encodeURIComponent(source)}/oauth/microsoft/authorize`,
        { redirect_uri: microsoftOAuthRedirectUri() }
      );
      window.open(response.authorization_url, "_self", "noopener");
    } catch (error) {
      showError(error);
      setOauthLoading(false);
    }
  }

  async function runConnector(mode: string) {
    if (!selected || !configEditable) {
      return;
    }
    const runSource = selected.source;
    setRunLoading(true);
    try {
      const response = await client.post<ConnectorRunExecutionResponse>(
        `/connectors/${encodeURIComponent(runSource)}/runs`,
        { mode }
      );
      notifications.show({
        title: `Run ${response.run.status}`,
        message: `${response.source} / ${response.target}`,
        color: response.run.status === "failed" ? "red" : "teal"
      });
      await loadConnectorDetails(runSource, { preserveRunDetail: true });
      await loadOperations();
      await loadRunDetail(response.run.id, { source: runSource });
      scheduleRunRefresh(runSource, response.run.id);
    } catch (error) {
      showError(error);
    } finally {
      setRunLoading(false);
    }
  }

  async function retryRun(run: ConnectorRun) {
    if (!run) {
      return;
    }
    const runSource = run.source;
    setRetryingRunId(run.id);
    try {
      const response = await client.post<ConnectorRunExecutionResponse>(
        `/connectors/runs/${encodeURIComponent(run.id)}/retry`,
        {}
      );
      notifications.show({
        title: "Retry queued",
        message: `${response.source} / ${response.target}`,
        color: "teal"
      });
      await loadConnectorDetails(runSource, { preserveRunDetail: true });
      await loadOperations();
      await loadRunDetail(response.run.id, { source: runSource });
      scheduleRunRefresh(runSource, response.run.id);
    } catch (error) {
      showError(error);
    } finally {
      setRetryingRunId(null);
    }
  }

  async function cancelRun(run: ConnectorRun) {
    setCancellingRunId(run.id);
    try {
      const cancelled = await client.post<ConnectorRun>(
        `/connectors/runs/${encodeURIComponent(run.id)}/cancel`,
        {}
      );
      notifications.show({
        title: cancelled.status === "cancelled" ? "Run cancelled" : "Cancellation requested",
        message: `${cancelled.source} / ${cancelled.target}`,
        color: "orange"
      });
      await loadConnectorDetails(run.source, { preserveRunDetail: true });
      await loadOperations();
      await loadRunDetail(run.id, { source: run.source });
      if (cancelled.status === "running") {
        scheduleRunRefresh(run.source, run.id);
      }
    } catch (error) {
      showError(error);
    } finally {
      setCancellingRunId(null);
    }
  }

  function applyTemplate(templateId: string) {
    const nextConfig = connectorConfigFromTemplate(templateId);

    if (nextConfig) {
      setConfig(nextConfig);
    }
  }

  return (
    <ViewFrame
      eyebrow="Runtime"
      title="Connectors"
      loading={initialLoading}
      loadingFallback={<ConnectorsSkeleton />}
      actions={
        <Button
          leftSection={<IconPlus size={16} />}
          onClick={createModal.open}
          disabled={initialLoading}
        >
          Create connector
        </Button>
      }
    >
      <Modal
        opened={createOpened}
        onClose={createModal.close}
        title="Create connector"
        size="lg"
        centered
      >
        <ConnectorCreateForm
          onCreate={createConnector}
          onCancel={createModal.close}
          submitting={creating}
          maintainers={maintainers}
          users={users}
          scopeOptionsLoading={scopeOptionsLoading}
          scopeOptionsError={scopeOptionsError}
        />
      </Modal>

      <Modal
        opened={scopeOpened}
        onClose={scopeModal.close}
        title="Edit connector visibility"
        size="md"
        centered
      >
        {selected && (
          <ConnectorScopeForm
            key={`${selected.source}-${selected.scope_type}-${selected.owner_user_id}-${selected.maintainer_id}`}
            connector={selected}
            maintainers={maintainers}
            users={users}
            optionsLoading={scopeOptionsLoading}
            optionsError={scopeOptionsError}
            saving={scopeSaving}
            onSave={updateConnectorScope}
            onCancel={scopeModal.close}
          />
        )}
      </Modal>

      <Stack gap="md">
        <ConnectorOperationsPanel operations={operations} />

        <Grid align="stretch" className="connectorWorkspaceGrid">
          <Grid.Col span={{ base: 12, md: 4 }} className="connectorRegistryCol">
            <ConnectorRegistry
              connectors={connectors}
              selectedSource={selectedSource}
              onSelect={selectConnector}
            />
          </Grid.Col>

          <Grid.Col span={{ base: 12, md: 8 }} className="connectorWorkspaceMain">
            <Stack gap="md" className="connectorWorkspaceStack">
              <ConnectorConfigEditor
                selected={selected}
                config={config}
                configLoadState={configLoadState}
                configLoadError={configLoadError}
                onConfigChange={setConfig}
                onRetryConfig={() =>
                  selected && loadConnectorDetails(selected.source, { preserveRunDetail: true })
                }
                onRun={runConnector}
                onSave={saveConfig}
                onConnectMicrosoft={connectMicrosoft}
                onEditScope={scopeModal.open}
                onApplyTemplate={applyTemplate}
                runLoading={runLoading}
                oauthLoading={oauthLoading}
                saving={saving}
              />
              <ConnectorRunsPanel
                runs={runs}
                runDetail={runDetail}
                runDetailLoading={runDetailLoading}
                onSelectRun={loadRunDetail}
                onRetryRun={retryRun}
                retryingRunId={retryingRunId}
                onCancelRun={cancelRun}
                cancellingRunId={cancellingRunId}
                onOpenService={onOpenService}
              />
            </Stack>
          </Grid.Col>
        </Grid>
      </Stack>
    </ViewFrame>
  );
}

function microsoftOAuthRedirectUri(): string {
  return `${window.location.origin}/oauth/microsoft/callback`;
}

function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}
