import {
  Alert,
  Box,
  Button,
  Checkbox,
  Grid,
  Group,
  Paper,
  Select,
  Stack,
  Text,
  Textarea,
  TextInput,
  Title
} from "@mantine/core";
import {
  IconAlertTriangle,
  IconBolt,
  IconPlayerPlay,
  IconPlugConnected,
  IconRefresh,
  IconUsers,
  IconTemplate
} from "@tabler/icons-react";
import type { Dispatch, FormEvent, SetStateAction } from "react";

import type { Connector, ConnectorConfigForm } from "../../types/api";
import { connectorConfigDiagnostics, connectorTemplates } from "./connectorConfig";
import type { ConnectorConfigLoadState } from "./connectorConfig";

const templateOptions = connectorTemplates.map((template) => ({
  value: template.id,
  label: template.label
}));

export function ConnectorConfigEditor({
  selected,
  config,
  configLoadState,
  configLoadError,
  onConfigChange,
  onRetryConfig,
  onRun,
  onSave,
  onConnectMicrosoft,
  onEditScope,
  onApplyTemplate,
  runLoading,
  oauthLoading,
  saving
}: {
  selected?: Connector;
  config: ConnectorConfigForm;
  configLoadState: ConnectorConfigLoadState;
  configLoadError: Error | null;
  onConfigChange: Dispatch<SetStateAction<ConnectorConfigForm>>;
  onRetryConfig: () => void | Promise<void>;
  onRun: (mode: string) => void | Promise<void>;
  onSave: (event: FormEvent<HTMLFormElement>) => void | Promise<void>;
  onConnectMicrosoft: () => void | Promise<void>;
  onEditScope: () => void;
  onApplyTemplate: (templateId: string) => void;
  runLoading: boolean;
  oauthLoading: boolean;
  saving: boolean;
}) {
  function updateConfig<K extends keyof ConnectorConfigForm>(field: K, value: ConnectorConfigForm[K]) {
    onConfigChange((current) => ({ ...current, [field]: value }));
  }

  const graphOAuthState = microsoftGraphOAuthState(config.config);
  const configDiagnostics = connectorConfigDiagnostics(config);
  const hasConfigErrors = configDiagnostics.some((diagnostic) => diagnostic.level === "error");
  const editorLocked =
    !selected ||
    configLoadState === "idle" ||
    configLoadState === "loading" ||
    configLoadState === "error";

  return (
    <Paper p="md" withBorder>
      <Group justify="space-between" align="flex-start" mb="md" className="panelHeader">
        <Box>
          <Title order={2} size="h3">
            {selected?.display_name || "Connector"}
          </Title>
          {selected && (
            <Text size="sm" c="dimmed">
              {selected.source}
            </Text>
          )}
        </Box>
        <Group className="responsiveActions">
          <Button
            type="button"
            variant="default"
            leftSection={<IconUsers size={16} />}
            disabled={!selected}
            onClick={onEditScope}
          >
            Edit visibility
          </Button>
          {graphOAuthState.enabled && (
            <Button
              type="button"
              variant="light"
              leftSection={<IconPlugConnected size={16} />}
              disabled={editorLocked}
              loading={oauthLoading}
              onClick={onConnectMicrosoft}
            >
              {graphOAuthState.connected ? "Reconnect Microsoft" : "Connect Microsoft"}
            </Button>
          )}
          <Button
            type="button"
            variant="default"
            leftSection={<IconPlayerPlay size={16} />}
            disabled={editorLocked}
            loading={runLoading}
            onClick={() => onRun("execute")}
          >
            Execute
          </Button>
          <Button
            type="button"
            leftSection={<IconBolt size={16} />}
            disabled={editorLocked}
            loading={runLoading}
            onClick={() => onRun("queue")}
          >
            Queue
          </Button>
        </Group>
      </Group>

      {configLoadState === "error" && (
        <Alert
          color="red"
          icon={<IconAlertTriangle size={18} />}
          title="Connector config unavailable"
          variant="light"
          mb="md"
        >
          <Stack gap="xs">
            <Text size="sm">
              The saved configuration could not be loaded. Editing is locked to prevent
              overwriting it.
            </Text>
            {configLoadError && (
              <Text size="sm" c="red.8">
                {configLoadError.message}
              </Text>
            )}
            <Box>
              <Button
                type="button"
                variant="light"
                color="red"
                size="xs"
                leftSection={<IconRefresh size={14} />}
                onClick={onRetryConfig}
              >
                Retry config
              </Button>
            </Box>
          </Stack>
        </Alert>
      )}

      <form onSubmit={onSave}>
        <Stack>
          <Grid>
            <Grid.Col span={{ base: 12, md: 4 }}>
              <Select
                label="Template"
                placeholder="Apply template"
                data={templateOptions}
                value={null}
                leftSection={<IconTemplate size={16} />}
                onChange={(templateId) => templateId && onApplyTemplate(templateId)}
                disabled={editorLocked}
              />
            </Grid.Col>
            <Grid.Col span={{ base: 12, md: 5 }}>
              <Select
                label="Target"
                value={config.target}
                disabled={editorLocked}
                onChange={(target) => updateConfig("target", target || "")}
                data={["work_cards", "notifications", "calendar_events", "service_health"]}
              />
            </Grid.Col>
            <Grid.Col span={{ base: 12, md: 3 }}>
              <TextInput
                label="Schedule"
                value={config.schedule_cron}
                disabled={editorLocked}
                placeholder="@every 15m"
                onChange={(event) => updateConfig("schedule_cron", event.currentTarget.value)}
              />
            </Grid.Col>
            <Grid.Col span={{ base: 12, md: 12 }}>
              <Checkbox
                label="Enabled"
                checked={config.enabled}
                disabled={editorLocked}
                onChange={(event) => updateConfig("enabled", event.currentTarget.checked)}
              />
            </Grid.Col>
          </Grid>

          {configDiagnostics.length > 0 && (
            <Alert
              color={hasConfigErrors ? "red" : "yellow"}
              icon={<IconAlertTriangle size={18} />}
              title="Config checks"
              variant="light"
            >
              <Stack gap={4}>
                {configDiagnostics.map((diagnostic, index) => (
                  <Text key={`${diagnostic.message}-${index}`} size="sm">
                    {diagnostic.message}
                  </Text>
                ))}
              </Stack>
            </Alert>
          )}

          <Textarea
            label="Config JSON"
            minRows={8}
            autosize
            value={config.config}
            disabled={editorLocked}
            onChange={(event) => updateConfig("config", event.currentTarget.value)}
            classNames={{ input: "codeInput" }}
          />
          <Textarea
            label="Sample payload"
            minRows={8}
            autosize
            value={config.sample_payload}
            disabled={editorLocked}
            onChange={(event) => updateConfig("sample_payload", event.currentTarget.value)}
            classNames={{ input: "codeInput" }}
          />
          <Group justify="flex-end">
            <Button type="submit" disabled={editorLocked} loading={saving}>
              Save config
            </Button>
          </Group>
        </Stack>
      </form>
    </Paper>
  );
}

function microsoftGraphOAuthState(configJson: string): { enabled: boolean; connected: boolean } {
  try {
    const parsed = JSON.parse(configJson) as { adapter?: unknown; refresh_token?: unknown };
    const adapter = typeof parsed.adapter === "string" ? parsed.adapter : "";
    const enabled = [
      "microsoft_graph_calendar",
      "graph_calendar",
      "outlook_calendar",
      "microsoft_graph_mail",
      "graph_mail",
      "outlook_mail"
    ].includes(adapter);
    const refreshToken = typeof parsed.refresh_token === "string" ? parsed.refresh_token.trim() : "";

    return { enabled, connected: enabled && refreshToken.length > 0 };
  } catch {
    return { enabled: false, connected: false };
  }
}
