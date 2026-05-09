import { NumberFormatter, Paper, Text, Title } from "@mantine/core";

export function Metric({ label, value }) {
  return (
    <Paper p={{ base: "sm", sm: "md" }} withBorder>
      <Text size="sm" c="dimmed">
        {label}
      </Text>
      <Title order={2} mt={6}>
        <NumberFormatter value={value || 0} thousandSeparator />
      </Title>
    </Paper>
  );
}
