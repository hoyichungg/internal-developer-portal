export const SESSION_EVENT_STORAGE_KEY = "idp_session_event";
const LEGACY_CREDENTIAL_STORAGE_KEYS = ["idp_token", "idp_token_expires_at"];

export type SessionEvent = "signed-in" | "signed-out";

export function clearLegacySessionCredentials() {
  for (const key of LEGACY_CREDENTIAL_STORAGE_KEYS) {
    try {
      window.localStorage.removeItem(key);
    } catch {
      // Storage can be unavailable in restricted browser contexts.
    }
  }
}

export function publishSessionEvent(event: SessionEvent) {
  try {
    window.localStorage.setItem(SESSION_EVENT_STORAGE_KEY, event);
    window.localStorage.removeItem(SESSION_EVENT_STORAGE_KEY);
  } catch {
    // Cross-tab synchronization is best effort; the HttpOnly cookie remains authoritative.
  }
}

export function subscribeToSessionEvents(listener: (event: SessionEvent) => void) {
  function handleStorage(event: StorageEvent) {
    if (event.storageArea && event.storageArea !== window.localStorage) {
      return;
    }
    if (event.key !== SESSION_EVENT_STORAGE_KEY || !isSessionEvent(event.newValue)) {
      return;
    }

    listener(event.newValue);
  }

  window.addEventListener("storage", handleStorage);
  return () => window.removeEventListener("storage", handleStorage);
}

function isSessionEvent(value: string | null): value is SessionEvent {
  return value === "signed-in" || value === "signed-out";
}
