import { Alert, Button, Stack, Text } from "@mantine/core";
import { IconLock } from "@tabler/icons-react";

import { ViewFrame } from "./ViewFrame";

export function AccessDenied({ onGoHome }: { onGoHome: () => void }) {
  return (
    <ViewFrame eyebrow="Access control" title="Permission required">
      <Alert color="yellow" icon={<IconLock size={18} />} title="You cannot open this view">
        <Stack gap="sm">
          <Text size="sm">
            Your account does not have the capability required for this administrative view.
          </Text>
          <Button variant="light" w="fit-content" onClick={onGoHome}>
            Return to dashboard
          </Button>
        </Stack>
      </Alert>
    </ViewFrame>
  );
}
