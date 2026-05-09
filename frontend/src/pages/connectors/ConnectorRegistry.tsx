import { NavLink, Stack } from "@mantine/core";

import { DataPanel } from "../../components/DataPanel";
import { EmptyText } from "../../components/EmptyText";
import { StatusBadge } from "../../components/tableCells";

export function ConnectorRegistry({ connectors, selectedSource, onSelect }) {
  return (
    <DataPanel title="Registry">
      <Stack gap={4}>
        {connectors.length === 0 && <EmptyText>No connectors</EmptyText>}
        {connectors.map((connector) => (
          <NavLink
            key={connector.source}
            active={connector.source === selectedSource}
            label={connector.display_name}
            description={`${connector.source} - ${connector.kind}`}
            rightSection={<StatusBadge value={connector.status} />}
            onClick={() => onSelect(connector.source)}
          />
        ))}
      </Stack>
    </DataPanel>
  );
}
