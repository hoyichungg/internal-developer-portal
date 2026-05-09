import { Badge, Button, Code, Text } from "@mantine/core";

export function StatusBadge({ value, className }) {
  if (!value) {
    return null;
  }

  const status = String(value).trim().toLowerCase();
  const colorMap = {
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
    <Badge variant="light" color={colorMap[status] || "gray"} className={classes} title={value}>
      {status.replaceAll("_", " ")}
    </Badge>
  );
}

export function DateCell({ value }) {
  if (!value) {
    return null;
  }

  return <Text size="sm">{new Date(value).toLocaleString()}</Text>;
}

export function LinkCell({ value }) {
  if (!value) {
    return null;
  }

  return (
    <Button component="a" href={value} target="_blank" rel="noreferrer" size="compact-sm" variant="subtle">
      Open
    </Button>
  );
}

export function MetadataCell({ value }) {
  if (!value) {
    return null;
  }

  return (
    <Code block className="metadataCell">
      {String(value)}
    </Code>
  );
}
