import { ScrollArea, Table } from "@mantine/core";

import { EmptyText } from "./EmptyText.jsx";
import { formatValue } from "../utils/format.js";

export function DataTable({ rows, columns }) {
  if (!rows || rows.length === 0) {
    return <EmptyText>No records</EmptyText>;
  }

  return (
    <ScrollArea>
      <Table striped highlightOnHover verticalSpacing="sm">
        <Table.Thead>
          <Table.Tr>
            {columns.map(([, label]) => (
              <Table.Th key={label}>{label}</Table.Th>
            ))}
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {rows.map((row, index) => (
            <Table.Tr key={row.id || `${row.source || "row"}-${index}`}>
              {columns.map(([key, label, Renderer]) => (
                <Table.Td key={`${key}-${label}`}>
                  {Renderer ? <Renderer value={row[key]} row={row} /> : formatValue(row[key])}
                </Table.Td>
              ))}
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
    </ScrollArea>
  );
}
