import { describe, expect, it } from "vitest";

import { isOffsetAwareRfc3339, toUtcRfc3339 } from "./dateTime";

describe("offset-aware RFC3339 helpers", () => {
  it.each([
    "2026-07-10T01:00:00Z",
    "2026-07-10T09:00:00+08:00",
    "2026-07-09T20:30:00.125-04:30"
  ])("accepts an explicit UTC designator or numeric offset: %s", (value) => {
    expect(isOffsetAwareRfc3339(value)).toBe(true);
  });

  it.each([
    "2026-07-10T09:00:00",
    "2026-07-10 09:00:00Z",
    "2026-07-10T09:00Z",
    "not-a-date"
  ])("rejects ambiguous or malformed timestamps: %s", (value) => {
    expect(isOffsetAwareRfc3339(value)).toBe(false);
  });

  it("normalizes an offset-aware instant to UTC", () => {
    expect(toUtcRfc3339(new Date("2026-07-10T09:00:00+08:00"))).toBe(
      "2026-07-10T01:00:00.000Z"
    );
  });
});
