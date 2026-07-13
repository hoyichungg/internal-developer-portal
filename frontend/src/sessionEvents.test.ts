import { fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  clearLegacySessionCredentials,
  publishSessionEvent,
  SESSION_EVENT_STORAGE_KEY,
  subscribeToSessionEvents
} from "./sessionEvents";

describe("sessionEvents", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it.each(["signed-in", "signed-out"] as const)(
    "publishes only the non-secret %s event and removes it immediately",
    (event) => {
      const setItem = vi.spyOn(Storage.prototype, "setItem");
      const removeItem = vi.spyOn(Storage.prototype, "removeItem");

      publishSessionEvent(event);

      expect(setItem).toHaveBeenCalledTimes(1);
      expect(setItem).toHaveBeenCalledWith(SESSION_EVENT_STORAGE_KEY, event);
      expect(removeItem).toHaveBeenCalledWith(SESSION_EVENT_STORAGE_KEY);
      expect(window.localStorage.getItem(SESSION_EVENT_STORAGE_KEY)).toBeNull();
    }
  );

  it("ignores unrelated or invalid storage events and unsubscribes cleanly", () => {
    const listener = vi.fn();
    const unsubscribe = subscribeToSessionEvents(listener);

    fireEvent(window, new StorageEvent("storage", { key: "other", newValue: "signed-in" }));
    fireEvent(
      window,
      new StorageEvent("storage", {
        key: SESSION_EVENT_STORAGE_KEY,
        newValue: "token-shaped-but-invalid"
      })
    );
    fireEvent(
      window,
      new StorageEvent("storage", {
        key: SESSION_EVENT_STORAGE_KEY,
        newValue: "signed-in"
      })
    );

    expect(listener).toHaveBeenCalledOnce();
    expect(listener).toHaveBeenCalledWith("signed-in");

    unsubscribe();
    fireEvent(
      window,
      new StorageEvent("storage", {
        key: SESSION_EVENT_STORAGE_KEY,
        newValue: "signed-out"
      })
    );
    expect(listener).toHaveBeenCalledOnce();
  });

  it("purges legacy credentials without touching the session event channel", () => {
    window.localStorage.setItem("idp_token", "legacy-secret");
    window.localStorage.setItem("idp_token_expires_at", "2099-01-01T00:00:00Z");
    window.localStorage.setItem(SESSION_EVENT_STORAGE_KEY, "signed-in");

    clearLegacySessionCredentials();

    expect(window.localStorage.getItem("idp_token")).toBeNull();
    expect(window.localStorage.getItem("idp_token_expires_at")).toBeNull();
    expect(window.localStorage.getItem(SESSION_EVENT_STORAGE_KEY)).toBe("signed-in");
  });
});
