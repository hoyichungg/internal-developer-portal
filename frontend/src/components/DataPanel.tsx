import { Group, Paper, Title } from "@mantine/core";

export function DataPanel({ title, children, className = undefined, actions = undefined }) {
  return (
    <Paper p={{ base: "sm", sm: "md" }} withBorder className={className}>
      {actions ? (
        <Group justify="space-between" align="flex-start" mb="md" className="panelHeader">
          <Title order={2} size="h3">
            {title}
          </Title>
          <Group className="responsiveActions">{actions}</Group>
        </Group>
      ) : (
        <Title order={2} size="h3" mb="md">
          {title}
        </Title>
      )}
      {children}
    </Paper>
  );
}
