const OFFSET_AWARE_RFC3339 =
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;

export function isOffsetAwareRfc3339(value: unknown): value is string {
  return (
    typeof value === "string" &&
    OFFSET_AWARE_RFC3339.test(value) &&
    Number.isFinite(Date.parse(value))
  );
}

export function toUtcRfc3339(value: Date): string {
  if (!Number.isFinite(value.getTime())) {
    throw new Error("A valid date is required.");
  }

  return value.toISOString();
}
