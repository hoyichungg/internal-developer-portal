import { ActionIcon, Tooltip, useComputedColorScheme, useMantineColorScheme } from "@mantine/core";
import { IconMoon, IconSun } from "@tabler/icons-react";

export function ThemeToggle() {
  const { setColorScheme } = useMantineColorScheme();
  const computedColorScheme = useComputedColorScheme("light", {
    getInitialValueInEffect: true
  });
  const isDark = computedColorScheme === "dark";

  return (
    <Tooltip label={isDark ? "Use light theme" : "Use dark theme"}>
      <ActionIcon
        variant="light"
        size="lg"
        aria-label={isDark ? "Use light theme" : "Use dark theme"}
        onClick={() => setColorScheme(isDark ? "light" : "dark")}
      >
        {isDark ? <IconSun size={18} /> : <IconMoon size={18} />}
      </ActionIcon>
    </Tooltip>
  );
}
