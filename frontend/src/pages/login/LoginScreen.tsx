import {
  Alert,
  Box,
  Button,
  Divider,
  Group,
  Loader,
  Paper,
  PasswordInput,
  Stack,
  Text,
  TextInput,
  Title
} from "@mantine/core";
import { useState } from "react";
import type { FormEvent } from "react";

import { CenterStage } from "../../components/CenterStage";
import { ThemeToggle } from "../../components/ThemeToggle";
import type { LoginRequest, PublicAuthConfig } from "../../types/api";

export function LoginScreen({
  authConfig,
  authConfigError,
  callbackError,
  entraLoginUrl,
  onLogin,
  onRetryAuthConfig
}: {
  authConfig: PublicAuthConfig | null;
  authConfigError: Error | null;
  callbackError?: string | null;
  entraLoginUrl: string;
  onLogin: (credentials: LoginRequest) => Promise<void>;
  onRetryAuthConfig: () => void;
}) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSubmitting(true);
    setError("");

    try {
      await onLogin({ username, password });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <CenterStage>
      <Paper w="min(440px, 100%)" p="xl" shadow="lg" withBorder>
        <Stack gap="md">
          <Group justify="space-between" align="flex-start" wrap="nowrap">
            <Box>
              <Text size="xs" fw={800} c="teal.8" tt="uppercase">
                Internal Developer Portal
              </Text>
              <Title order={1} size="h2">
                Engineering operations
              </Title>
            </Box>
            <ThemeToggle />
          </Group>

          {error && (
            <Alert color="red" title="Sign in failed">
              {error}
            </Alert>
          )}

          {callbackError && (
            <Alert color="red" title="Microsoft sign-in failed">
              {callbackError}
            </Alert>
          )}

          {authConfigError ? (
            <Alert color="yellow" title="Sign-in options unavailable">
              <Stack gap="sm">
                <Text size="sm">{authConfigError.message}</Text>
                <Button variant="light" color="yellow" onClick={onRetryAuthConfig}>
                  Retry
                </Button>
              </Stack>
            </Alert>
          ) : !authConfig ? (
            <Group role="status" gap="sm">
              <Loader size="sm" />
              <Text size="sm" c="dimmed">
                Loading sign-in options...
              </Text>
            </Group>
          ) : !authConfig.password_login_enabled && !authConfig.entra_login_enabled ? (
            <Alert color="red" title="Sign-in is not configured">
              No sign-in method is enabled. Contact the portal administrator.
            </Alert>
          ) : (
            <Stack gap="md">
              {authConfig.entra_login_enabled && (
                <Button component="a" href={entraLoginUrl} size="md">
                  Continue with Microsoft
                </Button>
              )}

              {authConfig.entra_login_enabled && authConfig.password_login_enabled && (
                <Divider label="or" labelPosition="center" />
              )}

              {authConfig.password_login_enabled && (
                <form onSubmit={submit}>
                  <Stack>
                    <TextInput
                      label="Username"
                      value={username}
                      onChange={(event) => setUsername(event.currentTarget.value)}
                      autoComplete="username"
                      required
                    />
                    <PasswordInput
                      label="Password"
                      value={password}
                      onChange={(event) => setPassword(event.currentTarget.value)}
                      autoComplete="current-password"
                      required
                    />
                    <Button type="submit" loading={submitting}>
                      Sign in with password
                    </Button>
                  </Stack>
                </form>
              )}
            </Stack>
          )}
        </Stack>
      </Paper>
    </CenterStage>
  );
}
