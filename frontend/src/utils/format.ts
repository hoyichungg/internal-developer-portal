export function formatValue(value) {
  if (value === null || value === undefined) {
    return "";
  }
  return String(value);
}

export function prettyJson(value) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value || "";
  }
}
