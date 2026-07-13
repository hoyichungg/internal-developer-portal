import { describe, expect, it } from "vitest";

import {
  entraLoginStartUrl,
  hasAuthCallbackParameters,
  parseEntraAuthCallback,
  safeReturnToFromHash,
  urlWithoutAuthCallbackParameters
} from "./authRouting";

describe("Entra auth routing", () => {
  it.each([
    ["#catalog", "/#catalog"],
    ["#/audit", "/#audit"],
    [
      "#my-work?source=azure-devops&due=next_7_days&sort=source_updated_desc&page=2&token=secret",
      "/#my-work?due=next_7_days&source=azure-devops&sort=source_updated_desc&page=2"
    ],
    ["#work-cards/42", "/#work-cards/42"],
    [
      "#work-cards/42?from=my-work&status=blocked&project=Portal&redirect=https://evil.test",
      "/#work-cards/42?from=my-work&status=blocked&project=Portal"
    ],
    ["#work-cards?id=42", "/#work-cards/42"],
    ["#notifications/7", "/#notifications/7"],
    [
      "#connectors?source=graph mail&target=notifications&run_id=9&ignored=secret",
      "/#connectors?source=graph+mail&target=notifications&runId=9"
    ]
  ])("preserves the safe portal route %s", (hash, expected) => {
    expect(safeReturnToFromHash(hash)).toBe(expected);
  });

  it.each([
    "",
    "#https://evil.example",
    "#//evil.example",
    "#work-cards/-1",
    "#notifications/not-a-number",
    `#catalog${"x".repeat(2_100)}`,
    "#catalog\nSet-Cookie:bad"
  ])("falls back to the dashboard for an unsafe route", (hash) => {
    expect(safeReturnToFromHash(hash)).toBe("/#dashboard");
  });

  it("encodes the safe return route in the fixed same-origin start path", () => {
    expect(entraLoginStartUrl("#catalog")).toBe(
      "/auth/entra/start?return_to=%2F%23catalog"
    );
  });

  it("accepts only the success marker and whitelisted error messages", () => {
    expect(parseEntraAuthCallback("?auth_result=entra")).toEqual({ kind: "success" });
    expect(parseEntraAuthCallback("?auth_result=other")).toBeNull();
    expect(parseEntraAuthCallback("?code=secret&state=secret")).toBeNull();
    expect(parseEntraAuthCallback("?auth_error=entra_access_denied")).toEqual({
      kind: "error",
      code: "entra_access_denied",
      message: "Microsoft sign-in was cancelled or denied."
    });
    expect(parseEntraAuthCallback("?auth_error=<script>secret</script>")).toEqual({
      kind: "error",
      code: null,
      message: "Microsoft sign-in could not be completed. Try again."
    });
  });

  it("removes callback and OAuth parameters while preserving route and unrelated query", () => {
    expect(
      urlWithoutAuthCallbackParameters(
        "https://portal.example/?theme=compact&auth_error=entra_invalid_state&code=secret&state=secret#catalog"
      )
    ).toBe("/?theme=compact#catalog");
  });

  it("detects callback secrets even without an Entra result marker", () => {
    expect(hasAuthCallbackParameters("?theme=compact&code=secret&state=secret")).toBe(true);
    expect(hasAuthCallbackParameters("?theme=compact")).toBe(false);
  });
});
