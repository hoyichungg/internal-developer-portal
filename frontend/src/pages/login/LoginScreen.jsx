import {
  Alert,
  Box,
  Button,
  Group,
  Paper,
  PasswordInput,
  Stack,
  Text,
  TextInput,
  Title
} from "@mantine/core";
import { useState } from "react";

import { CenterStage } from "../../components/CenterStage.jsx";
import { ThemeToggle } from "../../components/ThemeToggle.jsx";

export function LoginScreen({ onLogin }) {
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  async function submit(event) {
    event.preventDefault();
    setSubmitting(true);
    setError("");

    try {
      await onLogin({ username, password });
    } catch (error) {
      setError(error.message);
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
                Sign in
              </Button>
            </Stack>
          </form>
        </Stack>
      </Paper>
    </CenterStage>
  );
}
