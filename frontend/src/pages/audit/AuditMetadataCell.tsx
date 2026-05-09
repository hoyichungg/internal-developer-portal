import {
  ActionIcon,
  Badge,
  Box,
  Code,
  Group,
  Modal,
  Stack,
  Table,
  Text,
  Tooltip
} from "@mantine/core";
import { IconEye } from "@tabler/icons-react";
import { useState } from "react";

export function AuditMetadataCell({ value }) {
  const [opened, setOpened] = useState(false);

  if (!value) {
    return <Text c="dimmed">No metadata</Text>;
  }

  const metadata = parseMetadata(value);
  const entries = metadataEntries(metadata.parsed);
  const previewEntries = entries.slice(0, 3);

  return (
    <>
      <Group gap="xs" wrap="nowrap" align="flex-start" className="metadataSummaryCell">
        <Stack gap={4} className="metadataSummary">
          {previewEntries.map(([key, entryValue]) => (
            <Box key={key} className="metadataKv">
              <Text size="xs" c="dimmed" className="metadataKey">
                {key}
              </Text>
              <Text size="xs" className="metadataValue">
                {compactValue(entryValue)}
              </Text>
            </Box>
          ))}
          {entries.length > previewEntries.length && (
            <Badge variant="light" color="gray" size="xs" w="fit-content">
              +{entries.length - previewEntries.length} fields
            </Badge>
          )}
        </Stack>

        <Tooltip label="View metadata">
          <ActionIcon
            variant="subtle"
            size="sm"
            aria-label="View metadata"
            onClick={() => setOpened(true)}
          >
            <IconEye size={16} />
          </ActionIcon>
        </Tooltip>
      </Group>

      <Modal
        opened={opened}
        onClose={() => setOpened(false)}
        title="Audit metadata"
        size="xl"
        centered
      >
        <Stack gap="md">
          <Box className="metadataDetailTable">
            <Table verticalSpacing="sm" striped>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Field</Table.Th>
                  <Table.Th>Value</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {entries.map(([key, entryValue]) => (
                  <Table.Tr key={key}>
                    <Table.Td>
                      <Code>{key}</Code>
                    </Table.Td>
                    <Table.Td className="metadataDetailValue">
                      {detailValue(entryValue)}
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </Box>

          <Box>
            <Text size="sm" fw={700} mb="xs">
              Raw JSON
            </Text>
            <Code block className="metadataRawJson">
              {metadata.pretty}
            </Code>
          </Box>
        </Stack>
      </Modal>
    </>
  );
}

function parseMetadata(value) {
  if (typeof value !== "string") {
    return {
      parsed: value,
      pretty: JSON.stringify(value, null, 2)
    };
  }

  try {
    const parsed = JSON.parse(value);
    return {
      parsed,
      pretty: JSON.stringify(parsed, null, 2)
    };
  } catch {
    return {
      parsed: { value },
      pretty: value
    };
  }
}

function metadataEntries(value) {
  if (Array.isArray(value)) {
    return value.map((entry, index) => [`[${index}]`, entry]);
  }

  if (value && typeof value === "object") {
    return Object.entries(value);
  }

  return [["value", value]];
}

function compactValue(value) {
  if (value === null) {
    return "null";
  }
  if (Array.isArray(value)) {
    return `array(${value.length})`;
  }
  if (typeof value === "object") {
    return "object";
  }
  return String(value);
}

function detailValue(value) {
  if (value && typeof value === "object") {
    return (
      <Code block className="metadataInlineJson">
        {JSON.stringify(value, null, 2)}
      </Code>
    );
  }

  return <Text size="sm">{compactValue(value)}</Text>;
}
