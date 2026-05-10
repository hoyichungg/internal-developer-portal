import { Button, Group } from "@mantine/core";
import { IconBook, IconBrandGithub, IconChartBar } from "@tabler/icons-react";

type QuickLinksValue = {
  repository_url?: string | null;
  dashboard_url?: string | null;
  runbook_url?: string | null;
};

export function QuickLinks({
  links,
  compact = true
}: {
  links?: QuickLinksValue | null;
  compact?: boolean;
}) {
  const items = [
    {
      label: "Repo",
      href: links?.repository_url,
      icon: IconBrandGithub
    },
    {
      label: "Dashboard",
      href: links?.dashboard_url,
      icon: IconChartBar
    },
    {
      label: "Runbook",
      href: links?.runbook_url,
      icon: IconBook
    }
  ].filter((item) => item.href);

  if (items.length === 0) {
    return null;
  }

  return (
    <Group gap="xs" wrap="wrap" className="quickLinks">
      {items.map((item) => (
        <Button
          key={item.label}
          component="a"
          href={item.href}
          target="_blank"
          rel="noreferrer"
          variant="light"
          size={compact ? "compact-sm" : "sm"}
          leftSection={<item.icon size={14} />}
          onClick={(event) => event.stopPropagation()}
        >
          {item.label}
        </Button>
      ))}
    </Group>
  );
}
