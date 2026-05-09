import {
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
import { IconBolt, IconPlayerPlay, IconTemplate } from "@tabler/icons-react";

import { connectorTemplates } from "./connectorConfig.js";

const templateOptions = connectorTemplates.map((template) => ({
  value: template.id,
  label: template.label
}));

export function ConnectorConfigEditor({
  selected,
  config,
  onConfigChange,
  onRun,
  onSave,
  onApplyTemplate,
  runLoading,
  saving
}) {
  function updateConfig(field, value) {
    onConfigChange((current) => ({ ...current, [field]: value }));
  }

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
            variant="default"
            leftSection={<IconPlayerPlay size={16} />}
            disabled={!selected}
            loading={runLoading}
            onClick={() => onRun("execute")}
          >
            Execute
          </Button>
          <Button
            leftSection={<IconBolt size={16} />}
            disabled={!selected}
            loading={runLoading}
            onClick={() => onRun("queue")}
          >
            Queue
          </Button>
        </Group>
      </Group>

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
                disabled={!selected}
              />
            </Grid.Col>
            <Grid.Col span={{ base: 12, md: 5 }}>
              <Select
                label="Target"
                value={config.target}
                onChange={(target) => updateConfig("target", target)}
                data={["work_cards", "notifications", "service_health"]}
              />
            </Grid.Col>
            <Grid.Col span={{ base: 12, md: 3 }}>
              <TextInput
                label="Schedule"
                value={config.schedule_cron}
                placeholder="@every 15m"
                onChange={(event) => updateConfig("schedule_cron", event.currentTarget.value)}
              />
            </Grid.Col>
            <Grid.Col span={{ base: 12, md: 12 }}>
              <Checkbox
                label="Enabled"
                checked={config.enabled}
                onChange={(event) => updateConfig("enabled", event.currentTarget.checked)}
              />
            </Grid.Col>
          </Grid>

          <Textarea
            label="Config JSON"
            minRows={8}
            autosize
            value={config.config}
            onChange={(event) => updateConfig("config", event.currentTarget.value)}
            classNames={{ input: "codeInput" }}
          />
          <Textarea
            label="Sample payload"
            minRows={8}
            autosize
            value={config.sample_payload}
            onChange={(event) => updateConfig("sample_payload", event.currentTarget.value)}
            classNames={{ input: "codeInput" }}
          />
          <Group justify="flex-end">
            <Button type="submit" disabled={!selected} loading={saving}>
              Save config
            </Button>
          </Group>
        </Stack>
      </form>
    </Paper>
  );
}
