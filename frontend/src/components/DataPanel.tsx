import { Paper, Title } from "@mantine/core";

export function DataPanel({ title, children }) {
  return (
    <Paper p={{ base: "sm", sm: "md" }} withBorder>
      <Title order={2} size="h3" mb="md">
        {title}
      </Title>
      {children}
    </Paper>
  );
}
