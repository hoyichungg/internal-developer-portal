import { Badge, Button, Code, Text } from "@mantine/core";

type CellValue = string | number | boolean | null | undefined;

export function StatusBadge({ value, className }: { value?: CellValue; className?: string }) {
  if (!value) {
    return null;
  }

  const status = String(value).trim().toLowerCase();
  const colorMap: Record<string, string> = {
    active: "teal",
    archived: "gray",
    blocked: "orange",
    critical: "red",
    degraded: "yellow",
    deprecated: "orange",
    down: "red",
    done: "teal",
    error: "red",
    failed: "red",
    healthy: "teal",
    high: "orange",
    idle: "teal",
    in_progress: "blue",
    imported: "teal",
    low: "gray",
    maintainer: "blue",
    medium: "blue",
    missing: "red",
    none: "gray",
    owner: "teal",
    partial_success: "yellow",
    paused: "yellow",
    queued: "gray",
    retention_cleanup: "blue",
    running: "blue",
    stale: "yellow",
    success: "teal",
    todo: "gray",
    unknown: "gray",
    urgent: "red",
    viewer: "gray",
    warning: "yellow"
  };
  const classes = ["statusBadge", className].filter(Boolean).join(" ");

  return (
    <Badge variant="light" color={colorMap[status] || "gray"} className={classes} title={String(value)}>
      {status.replaceAll("_", " ")}
    </Badge>
  );
}

export function DateCell({ value }: { value?: CellValue }) {
  if (!value) {
    return null;
  }

  return <Text size="sm">{new Date(String(value)).toLocaleString()}</Text>;
}

export function LinkCell({ value }: { value?: CellValue }) {
  if (!value) {
    return null;
  }

  return (
    <Button component="a" href={String(value)} target="_blank" rel="noreferrer" size="compact-sm" variant="subtle">
      Open
    </Button>
  );
}

export function MetadataCell({ value }: { value?: unknown }) {
  if (!value) {
    return null;
  }

  return (
    <Code block className="metadataCell">
      {String(value)}
    </Code>
  );
}
