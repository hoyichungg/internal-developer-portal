import { ScrollArea, Table } from "@mantine/core";
import type { ReactNode } from "react";

import { EmptyText } from "./EmptyText";
import { formatValue } from "../utils/format";

type DataRow = object;
type CellRenderer<Row extends DataRow> = (props: { value: unknown; row: Row }) => ReactNode;
export type DataColumn<Row extends DataRow> = [key: Extract<keyof Row, string> | string, label: string, renderer?: CellRenderer<Row>];

export function DataTable<Row extends DataRow>({ rows, columns }: { rows?: Row[]; columns: DataColumn<Row>[] }) {
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
          {rows.map((row, index) => {
            const rowData = row as Record<string, unknown>;
            const rowKey = rowData.id || `${rowData.source || "row"}-${index}`;

            return (
              <Table.Tr key={String(rowKey)}>
                {columns.map(([key, label, Renderer]) => (
                  <Table.Td key={`${key}-${label}`}>
                    {Renderer ? (
                      <Renderer value={rowData[key]} row={row} />
                    ) : (
                      formatValue(rowData[key])
                    )}
                  </Table.Td>
                ))}
              </Table.Tr>
            );
          })}
        </Table.Tbody>
      </Table>
    </ScrollArea>
  );
}
