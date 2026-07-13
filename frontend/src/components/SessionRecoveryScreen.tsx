import { Alert, Button, Group, Paper, Stack, Text, Title } from "@mantine/core";
import { IconCloudOff } from "@tabler/icons-react";

import { CenterStage } from "./CenterStage";

export function SessionRecoveryScreen({
  error,
  onRetry,
  onSignOut
}: {
  error: Error;
  onRetry: () => void;
  onSignOut: () => void;
}) {
  return (
    <CenterStage>
      <Paper w="min(520px, 100%)" p="xl" shadow="lg" withBorder>
        <Stack gap="md">
          <div>
            <Text size="xs" fw={800} c="teal.8" tt="uppercase">
              Internal Developer Portal
            </Text>
            <Title order={1} size="h2">
              Session check unavailable
            </Title>
          </div>
          <Alert color="yellow" icon={<IconCloudOff size={18} />} title="Could not reach the portal API">
            <Stack gap="xs">
              <Text size="sm">
                Your browser session was not changed. Retry when the API or network is available
                again.
              </Text>
              <Text size="xs" c="dimmed">
                {error.message}
              </Text>
            </Stack>
          </Alert>
          <Group>
            <Button onClick={onRetry}>Retry</Button>
            <Button variant="subtle" color="gray" onClick={onSignOut}>
              Sign out on this device
            </Button>
          </Group>
        </Stack>
      </Paper>
    </CenterStage>
  );
}
