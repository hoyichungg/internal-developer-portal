export function formatValue(value: unknown) {
  if (value === null || value === undefined) {
    return "";
  }
  return String(value);
}

export function prettyJson(value: unknown) {
  try {
    return JSON.stringify(JSON.parse(String(value)), null, 2);
  } catch {
    return value ? String(value) : "";
  }
}
