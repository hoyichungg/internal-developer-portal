import { Button, Grid, Modal, Stack } from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import { IconPlus } from "@tabler/icons-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { ViewFrame } from "../../components/ViewFrame.jsx";
import { ConnectorsSkeleton } from "../../components/LoadingState.jsx";
import { useRefresh } from "../../hooks/useRefresh.js";
import { showError } from "../../utils/notifications.js";
import { ConnectorConfigEditor } from "./ConnectorConfigEditor.jsx";
import { ConnectorCreateForm } from "./ConnectorCreateForm.jsx";
import { ConnectorOperationsPanel } from "./ConnectorOperationsPanel.jsx";
import { ConnectorRegistry } from "./ConnectorRegistry.jsx";
import { ConnectorRunsPanel } from "./ConnectorRunsPanel.jsx";
import {
  connectorConfigFromResponse,
  connectorConfigFromTemplate,
  defaultConnectorConfig
} from "./connectorConfig.js";

export function ConnectorsView({ client, onOpenService }) {
  const [connectors, setConnectors] = useState([]);
  const [operations, setOperations] = useState(null);
  const [selectedSource, setSelectedSource] = useState("");
  const [config, setConfig] = useState(defaultConnectorConfig);
  const [runs, setRuns] = useState([]);
  const [runDetail, setRunDetail] = useState(null);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [saving, setSaving] = useState(false);
  const [runLoading, setRunLoading] = useState(false);
  const [retryingRunId, setRetryingRunId] = useState(null);
  const [runDetailLoading, setRunDetailLoading] = useState(false);
  const [createOpened, createModal] = useDisclosure(false);
  const selectedSourceRef = useRef("");
  const detailsRequestSeqRef = useRef(0);
  const reloadRequestSeqRef = useRef(0);
  const runDetailRequestSeqRef = useRef(0);
  const refreshTimeoutRef = useRef(null);
  const selected = connectors.find((connector) => connector.source === selectedSource);
  const initialLoading = loading && connectors.length === 0;

  useEffect(() => {
    selectedSourceRef.current = selectedSource;
  }, [selectedSource]);

  useEffect(() => () => {
    if (refreshTimeoutRef.current) {
      window.clearTimeout(refreshTimeoutRef.current);
    }
  }, []);

  const loadConnectorDetails = useCallback(
    async (source, options = {}) => {
      const requestSeq = ++detailsRequestSeqRef.current;
      try {
        if (!options.preserveRunDetail) {
          runDetailRequestSeqRef.current += 1;
          setRunDetail(null);
        }
        const [configResponse, runResponse] = await Promise.all([
          client.get(`/connectors/${encodeURIComponent(source)}/config`).catch(() => null),
          client.get(`/connectors/runs?source=${encodeURIComponent(source)}`)
        ]);
        if (requestSeq !== detailsRequestSeqRef.current) {
          return;
        }
        setRuns(runResponse);
        setConfig(connectorConfigFromResponse(configResponse));
      } catch (error) {
        if (requestSeq === detailsRequestSeqRef.current) {
          showError(error);
        }
      }
    },
    [client]
  );

  const loadOperations = useCallback(async () => {
    try {
      const nextOperations = await client.get("/connectors/operations");
      setOperations(nextOperations);
    } catch (error) {
      showError(error);
    }
  }, [client]);

  const loadRunDetail = useCallback(
    async (runId, options = {}) => {
      const requestSeq = ++runDetailRequestSeqRef.current;
      setRunDetailLoading(true);
      try {
        const detail = await client.get(`/connectors/runs/${encodeURIComponent(runId)}`);
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

  const reload = useCallback(async (preferredSource) => {
    const requestSeq = ++reloadRequestSeqRef.current;
    setLoading(true);
    try {
      const [nextConnectors, nextOperations] = await Promise.all([
        client.get("/connectors"),
        client.get("/connectors/operations")
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
        await loadConnectorDetails(nextSource);
      } else {
        detailsRequestSeqRef.current += 1;
        runDetailRequestSeqRef.current += 1;
        setConfig(defaultConnectorConfig);
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
  }, [client, loadConnectorDetails]);

  useEffect(() => {
    reload();
  }, []);
  useRefresh(() => reload());

  async function selectConnector(source) {
    if (refreshTimeoutRef.current) {
      window.clearTimeout(refreshTimeoutRef.current);
      refreshTimeoutRef.current = null;
    }
    selectedSourceRef.current = source;
    setSelectedSource(source);
    await loadConnectorDetails(source);
  }

  function scheduleRunRefresh(runSource, runId) {
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

  async function createConnector(event) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    setCreating(true);
    try {
      const connector = await client.post("/connectors", {
        source: form.get("source"),
        kind: form.get("kind"),
        display_name: form.get("display_name"),
        status: form.get("status")
      });
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

  async function saveConfig(event) {
    event.preventDefault();
    if (!selected) {
      return;
    }
    setSaving(true);
    try {
      JSON.parse(config.config);
      JSON.parse(config.sample_payload);
      await client.put(`/connectors/${encodeURIComponent(selected.source)}/config`, {
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

  async function runConnector(mode) {
    if (!selected) {
      return;
    }
    const runSource = selected.source;
    setRunLoading(true);
    try {
      const response = await client.post(`/connectors/${encodeURIComponent(runSource)}/runs`, {
        mode
      });
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

  async function retryRun(run) {
    if (!run) {
      return;
    }
    const runSource = run.source;
    setRetryingRunId(run.id);
    try {
      const response = await client.post(`/connectors/runs/${encodeURIComponent(run.id)}/retry`, {});
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

  function applyTemplate(templateId) {
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
        />
      </Modal>

      <Stack gap="md">
        <ConnectorOperationsPanel operations={operations} />

        <Grid>
          <Grid.Col span={{ base: 12, md: 4 }}>
            <ConnectorRegistry
              connectors={connectors}
              selectedSource={selectedSource}
              onSelect={selectConnector}
            />
          </Grid.Col>

          <Grid.Col span={{ base: 12, md: 8 }}>
            <Stack gap="md">
              <ConnectorConfigEditor
                selected={selected}
                config={config}
                onConfigChange={setConfig}
                onRun={runConnector}
                onSave={saveConfig}
                onApplyTemplate={applyTemplate}
                runLoading={runLoading}
                saving={saving}
              />
              <ConnectorRunsPanel
                runs={runs}
                runDetail={runDetail}
                runDetailLoading={runDetailLoading}
                onSelectRun={loadRunDetail}
                onRetryRun={retryRun}
                retryingRunId={retryingRunId}
                onOpenService={onOpenService}
              />
            </Stack>
          </Grid.Col>
        </Grid>
      </Stack>
    </ViewFrame>
  );
}
