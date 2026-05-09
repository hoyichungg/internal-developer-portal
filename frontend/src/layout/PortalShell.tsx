import {
  ActionIcon,
  AppShell,
  Box,
  Burger,
  Button,
  Group,
  Stack,
  Text,
  Tooltip
} from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { IconLogout, IconRefresh } from "@tabler/icons-react";

import { ThemeToggle } from "../components/ThemeToggle";
import { NAV_ITEMS } from "../navigation";

export function PortalShell({ user, view, onLogout, children }) {
  const [opened, { toggle }] = useDisclosure(false);

  return (
    <AppShell
      header={{ height: { base: 60, sm: 68 } }}
      navbar={{ width: 292, breakpoint: "sm", collapsed: { mobile: !opened } }}
      padding={{ base: "md", sm: "lg" }}
    >
      <AppShell.Header>
        <Group h="100%" px={{ base: "sm", sm: "lg" }} justify="space-between" wrap="nowrap">
          <Group gap="sm" wrap="nowrap" className="brandCluster">
            <Burger opened={opened} onClick={toggle} hiddenFrom="sm" size="sm" />
            <Box className="brandMark">IDP</Box>
            <Box className="brandCopy">
              <Text fw={800}>Developer Portal</Text>
              <Text size="xs" c="dimmed">
                {user.username} - {user.roles.join(", ")}
              </Text>
            </Box>
          </Group>
          <Group gap="xs" wrap="nowrap">
            <ThemeToggle />
            <Tooltip label="Refresh current view">
              <ActionIcon
                variant="light"
                size="lg"
                onClick={() => window.dispatchEvent(new CustomEvent("idp-refresh"))}
              >
                <IconRefresh size={18} />
              </ActionIcon>
            </Tooltip>
            <Button
              leftSection={<IconLogout size={16} />}
              variant="default"
              onClick={onLogout}
              visibleFrom="xs"
            >
              Sign out
            </Button>
            <Tooltip label="Sign out">
              <ActionIcon variant="default" size="lg" onClick={onLogout} hiddenFrom="xs">
                <IconLogout size={18} />
              </ActionIcon>
            </Tooltip>
          </Group>
        </Group>
      </AppShell.Header>

      <AppShell.Navbar p="md" className="appNavbar">
        <Stack gap={6}>
          {NAV_ITEMS.map((item) => (
            <a
              key={item.id}
              href={`#${item.id}`}
              aria-current={view === item.id ? "page" : undefined}
              className={`appNavButton${view === item.id ? " isActive" : ""}`}
            >
              <item.icon size={20} />
              <span>{item.label}</span>
            </a>
          ))}
        </Stack>
      </AppShell.Navbar>

      <AppShell.Main>
        <Box className="mainContent">{children}</Box>
      </AppShell.Main>
    </AppShell>
  );
}
