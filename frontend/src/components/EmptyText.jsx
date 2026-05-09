import { Text } from "@mantine/core";

export function EmptyText({ children }) {
  return (
    <Text c="dimmed" p="md" ta="center">
      {children}
    </Text>
  );
}
