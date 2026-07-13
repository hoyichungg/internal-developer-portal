import { safeMyWorkDetailQuery, safeMyWorkQuery } from "../work/myWorkRouting";

const DEFAULT_RETURN_TO = "/#dashboard";
const TOP_LEVEL_ROUTES = new Set(["dashboard", "my-work", "connectors", "catalog", "audit"]);
const AUTH_CALLBACK_QUERY_KEYS = [
  "auth_result",
  "auth_error",
  "code",
  "state",
  "session_state",
  "error",
  "error_description",
  "id_token",
  "access_token"
] as const;

const ENTRA_ERROR_MESSAGES = {
  entra_access_denied: "Microsoft sign-in was cancelled or denied.",
  entra_account_not_allowed: "This Microsoft account is not allowed to use the portal.",
  entra_invalid_state: "The Microsoft sign-in request expired or could not be verified. Try again.",
  entra_provider_unavailable: "Microsoft sign-in is temporarily unavailable. Try again later.",
  entra_configuration_error: "Microsoft sign-in is not configured correctly. Contact the portal administrator."
} as const;

export type EntraAuthErrorCode = keyof typeof ENTRA_ERROR_MESSAGES;

export type EntraAuthCallbackMarker =
  | { kind: "success" }
  | { kind: "error"; code: EntraAuthErrorCode | null; message: string };

export function safeReturnToFromHash(hash: string): string {
  // Keep this aligned with the backend OIDC transaction column and
  // normalize_return_to limit so a route accepted here is never silently
  // replaced with Dashboard after the identity-provider round trip.
  if (!hash || hash.length > 512 || /[\u0000-\u001f\u007f]/.test(hash)) {
    return DEFAULT_RETURN_TO;
  }

  const normalized = hash.replace(/^#\/?/, "");
  const [route, query = ""] = normalized.split("?", 2);

  if (route === "my-work") {
    const safeQuery = safeMyWorkQuery(query);
    return safeQuery ? `/#my-work?${safeQuery}` : "/#my-work";
  }

  if (TOP_LEVEL_ROUTES.has(route) && route !== "connectors") {
    return `/#${route}`;
  }

  if (route === "connectors") {
    const safeQuery = safeConnectorQuery(query);
    return safeQuery ? `/#connectors?${safeQuery}` : "/#connectors";
  }

  const workCardRouteId = positiveIntegerRouteId(route, "work-cards/");
  if (workCardRouteId !== null) {
    const safeQuery = safeMyWorkDetailQuery(query);
    return safeQuery
      ? `/#work-cards/${workCardRouteId}?${safeQuery}`
      : `/#work-cards/${workCardRouteId}`;
  }

  const workCardQueryId = positiveIntegerQueryId(route, query, "work-cards");
  if (workCardQueryId !== null) {
    return `/#work-cards/${workCardQueryId}`;
  }

  const notificationId =
    positiveIntegerRouteId(route, "notifications/") ??
    positiveIntegerQueryId(route, query, "notifications");
  if (notificationId !== null) {
    return `/#notifications/${notificationId}`;
  }

  return DEFAULT_RETURN_TO;
}

export function entraLoginStartUrl(hash: string): string {
  const params = new URLSearchParams({ return_to: safeReturnToFromHash(hash) });
  return `/auth/entra/start?${params.toString()}`;
}

export function parseEntraAuthCallback(search: string): EntraAuthCallbackMarker | null {
  const params = new URLSearchParams(search);
  const errorCode = params.get("auth_error");
  if (errorCode) {
    if (isEntraAuthErrorCode(errorCode)) {
      return { kind: "error", code: errorCode, message: ENTRA_ERROR_MESSAGES[errorCode] };
    }

    return {
      kind: "error",
      code: null,
      message: "Microsoft sign-in could not be completed. Try again."
    };
  }

  return params.get("auth_result") === "entra" ? { kind: "success" } : null;
}

export function hasAuthCallbackParameters(search: string): boolean {
  const params = new URLSearchParams(search);
  return AUTH_CALLBACK_QUERY_KEYS.some((key) => params.has(key));
}

export function urlWithoutAuthCallbackParameters(currentUrl: string): string {
  const url = new URL(currentUrl, "http://portal.invalid");
  for (const key of AUTH_CALLBACK_QUERY_KEYS) {
    url.searchParams.delete(key);
  }

  const query = url.searchParams.toString();
  return `${url.pathname}${query ? `?${query}` : ""}${url.hash}`;
}

function safeConnectorQuery(query: string): string {
  const incoming = new URLSearchParams(query);
  const outgoing = new URLSearchParams();

  copyBoundedValue(incoming, outgoing, "source");
  copyBoundedValue(incoming, outgoing, "target");

  const runId = positiveInteger(incoming.get("runId") ?? incoming.get("run_id"));
  if (runId !== null) {
    outgoing.set("runId", String(runId));
  }

  return outgoing.toString();
}

function copyBoundedValue(incoming: URLSearchParams, outgoing: URLSearchParams, key: string) {
  const value = incoming.get(key)?.trim();
  if (value && value.length <= 128 && !/[\u0000-\u001f\u007f]/.test(value)) {
    outgoing.set(key, value);
  }
}

function positiveIntegerRouteId(route: string, prefix: string): number | null {
  if (!route.startsWith(prefix)) {
    return null;
  }

  return positiveInteger(route.slice(prefix.length));
}

function positiveIntegerQueryId(route: string, query: string, expectedRoute: string): number | null {
  if (route !== expectedRoute) {
    return null;
  }

  return positiveInteger(new URLSearchParams(query).get("id"));
}

function positiveInteger(value: string | null): number | null {
  if (!value || !/^\d+$/.test(value)) {
    return null;
  }

  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null;
}

function isEntraAuthErrorCode(value: string): value is EntraAuthErrorCode {
  return Object.prototype.hasOwnProperty.call(ENTRA_ERROR_MESSAGES, value);
}
