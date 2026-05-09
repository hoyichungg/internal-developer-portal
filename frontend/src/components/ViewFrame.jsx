import { Alert, Box, Group, Stack, Text, Title } from "@mantine/core";

import { PageLoader } from "./LoadingState.jsx";

export function ViewFrame({
  eyebrow,
  title,
  backAction,
  actions,
  loading,
  loadingFallback,
  error,
  children
}) {
  return (
    <Stack gap="lg" className="viewFrame">
      <Stack gap={6} className="viewHeader">
        {(backAction || actions) && (
          <Box className="viewToolbar">
            {backAction && <Group className="viewBackAction">{backAction}</Group>}
            {actions && <Group className="viewActions">{actions}</Group>}
          </Box>
        )}
        <Box>
          <Text size="xs" fw={800} c="teal.8" tt="uppercase">
            {eyebrow}
          </Text>
          <Title order={1}>{title}</Title>
        </Box>
      </Stack>
      {loading && (loadingFallback || <PageLoader />)}
      {error && (
        <Alert color="red" title="Request failed">
          {error.message}
        </Alert>
      )}
      {!loading && !error && children}
    </Stack>
  );
}
