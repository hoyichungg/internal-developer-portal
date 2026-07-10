import { NavLink, ScrollArea, Stack, Text } from "@mantine/core";

import { DataPanel } from "../../components/DataPanel";
import { EmptyText } from "../../components/EmptyText";
import { StatusBadge } from "../../components/tableCells";
import type { Connector } from "../../types/api";

export function ConnectorRegistry({
  connectors,
  selectedSource,
  onSelect
}: {
  connectors: Connector[];
  selectedSource: string;
  onSelect: (source: string) => void | Promise<void>;
}) {
  return (
    <DataPanel
      title="Registry"
      className="connectorRegistryPanel"
      actions={
        connectors.length > 0 ? (
          <Text size="sm" c="dimmed">
            {connectors.length} connectors
          </Text>
        ) : undefined
      }
    >
      <ScrollArea offsetScrollbars className="connectorRegistryScroll">
        <Stack gap={4} className="connectorRegistryList">
          {connectors.length === 0 && <EmptyText>No connectors</EmptyText>}
          {connectors.map((connector) => (
            <NavLink
              key={connector.source}
              active={connector.source === selectedSource}
              className="connectorRegistryItem"
              label={
                <Text size="sm" fw={700} className="connectorRegistryTitle" title={connector.display_name}>
                  {connector.display_name}
                </Text>
              }
              description={
                <Text
                  size="xs"
                  c="dimmed"
                  className="connectorRegistryMeta"
                  title={`${connector.source} - ${connector.kind} - ${scopeLabel(connector)}`}
                >
                  {connector.source} - {connector.kind} - {scopeLabel(connector)}
                </Text>
              }
              rightSection={<StatusBadge value={connector.status} />}
              onClick={() => onSelect(connector.source)}
            />
          ))}
        </Stack>
      </ScrollArea>
    </DataPanel>
  );
}

function scopeLabel(connector: Connector): string {
  if (connector.scope_type === "user") return `private user #${connector.owner_user_id}`;
  if (connector.scope_type === "maintainer") return `team #${connector.maintainer_id}`;
  return "everyone";
}
