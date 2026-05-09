import { Paper, Title } from "@mantine/core";

export function DataPanel({ title, children, className = undefined }) {
  return (
    <Paper p={{ base: "sm", sm: "md" }} withBorder className={className}>
      <Title order={2} size="h3" mb="md">
        {title}
      </Title>
      {children}
    </Paper>
  );
}
