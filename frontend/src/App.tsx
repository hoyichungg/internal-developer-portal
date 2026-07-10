import { notifications } from "@mantine/notifications";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { createApiClient, isApiError } from "./api/client";
import { AccessDenied } from "./components/AccessDenied";
import { CenterStage } from "./components/CenterStage";
import { PageLoader } from "./components/LoadingState";
import { SessionRecoveryScreen } from "./components/SessionRecoveryScreen";
import { useStoredToken } from "./hooks/useStoredToken";
import { PortalShell } from "./layout/PortalShell";
import { AuditView } from "./pages/audit/AuditView";
import { CatalogView } from "./pages/catalog/CatalogView";
import { ConnectorsView } from "./pages/connectors/ConnectorsView";
import { DashboardView } from "./pages/dashboard/DashboardView";
import { LoginScreen } from "./pages/login/LoginScreen";
import { NotificationDetailView } from "./pages/records/NotificationDetailView";
import { WorkCardDetailView } from "./pages/records/WorkCardDetailView";
import { ServiceOverviewView } from "./pages/services/ServiceOverviewView";
import type {
  ApiId,
  ConnectorDrillTarget,
  LoginRequest,
  LoginResponse,
  MeResponse,
  MicrosoftOAuthCallbackResponse
} from "./types/api";

const TOP_LEVEL_VIEWS = new Set(["dashboard", "connectors", "catalog", "audit"]);
type AppView =
  | "dashboard"
  | "connectors"
  | "catalog"
  | "audit"
  | "service-overview"
  | "work-card-detail"
  | "notification-detail";

type AppRoute = {
  view: AppView;
  params: URLSearchParams;
  recordId: ApiId | null;
};

function routeFromHash(): AppRoute {
  const hash = window.location.hash.replace(/^#\/?/, "");
  const [viewPart, query = ""] = hash.split("?");
  const params = new URLSearchParams(query);

  if (viewPart.startsWith("work-cards/")) {
    const recordId = numericId(viewPart.replace("work-cards/", ""));
    return {
      view: recordId ? "work-card-detail" : "dashboard",
      params,
      recordId
    };
  }

  if (viewPart.startsWith("notifications/")) {
    const recordId = numericId(viewPart.replace("notifications/", ""));
    return {
      view: recordId ? "notification-detail" : "dashboard",
      params,
      recordId
    };
  }

  if (viewPart === "work-cards") {
    const recordId = numericId(params.get("id"));
    return {
      view: recordId ? "work-card-detail" : "dashboard",
      params,
      recordId
    };
  }

  if (viewPart === "notifications") {
    const recordId = numericId(params.get("id"));
    return {
      view: recordId ? "notification-detail" : "dashboard",
      params,
      recordId
    };
  }

  const view = TOP_LEVEL_VIEWS.has(viewPart) ? (viewPart as AppView) : "dashboard";

  return { view, params, recordId: null };
}

function numericId(value?: string | null): ApiId | null {
  const id = value ? Number(value) : NaN;
  return Number.isInteger(id) && id > 0 ? id : null;
}

function connectorTargetFromParams(params: URLSearchParams): ConnectorDrillTarget | null {
  const source = params.get("source");
  const target = params.get("target");
  const runParam = params.get("runId") || params.get("run_id");
  const runNumber = runParam ? Number(runParam) : null;
  const runId = runNumber !== null && Number.isFinite(runNumber) ? runNumber : null;

  if (!source && !target && !runId) {
    return null;
  }

  return {
    source,
    target,
    runId
  };
}

function connectorHash(target?: ConnectorDrillTarget | null): string {
  const params = new URLSearchParams();

  if (target?.source) {
    params.set("source", target.source);
  }
  if (target?.target) {
    params.set("target", target.target);
  }
  if (target?.runId) {
    params.set("runId", String(target.runId));
  }

  const query = params.toString();
  return query ? `#connectors?${query}` : "#connectors";
}

function microsoftOAuthCallbackParams(): URLSearchParams | null {
  if (window.location.pathname !== "/oauth/microsoft/callback") {
    return null;
  }

  return new URLSearchParams(window.location.search);
}

function microsoftOAuthRedirectUri(): string {
  return `${window.location.origin}/oauth/microsoft/callback`;
}

function clearMicrosoftOAuthCallbackUrl() {
  window.history.replaceState(null, "", "/");
}

function canAccessView(user: MeResponse, view: AppView): boolean {
  if (view === "connectors") {
    return user.capabilities.manage_connectors;
  }
  if (view === "audit") {
    return user.capabilities.view_audit;
  }

  return true;
}

export default function App() {
  const initialRoute = useMemo(() => routeFromHash(), []);
  const [token, setToken, storedExpiresAt] = useStoredToken();
  const [user, setUser] = useState<MeResponse | null>(null);
  const [view, setView] = useState(() => initialRoute.view);
  const [selectedServiceId, setSelectedServiceId] = useState<ApiId | null>(null);
  const [selectedWorkCardId, setSelectedWorkCardId] = useState<ApiId | null>(() =>
    initialRoute.view === "work-card-detail" ? initialRoute.recordId : null
  );
  const [selectedNotificationId, setSelectedNotificationId] = useState<ApiId | null>(() =>
    initialRoute.view === "notification-detail" ? initialRoute.recordId : null
  );
  const [connectorDrillTarget, setConnectorDrillTarget] =
    useState<ConnectorDrillTarget | null>(() => {
      return initialRoute.view === "connectors" ? connectorTargetFromParams(initialRoute.params) : null;
    });
  const [booting, setBooting] = useState(true);
  const [restoreError, setRestoreError] = useState<Error | null>(null);
  const [restoreAttempt, setRestoreAttempt] = useState(0);
  const restoreRequestIdRef = useRef(0);
  const handledMicrosoftOAuthCallbackRef = useRef("");

  const handleUnauthorized = useCallback(() => {
    setUser(null);
    setRestoreError(null);
    setBooting(false);
    setToken(null);
  }, [setToken]);

  const client = useMemo(
    () => createApiClient(token, { onUnauthorized: handleUnauthorized }),
    [handleUnauthorized, token]
  );

  useEffect(() => {
    function syncViewFromHash() {
      const route = routeFromHash();
      setView(route.view);
      setSelectedServiceId(null);
      setSelectedWorkCardId(route.view === "work-card-detail" ? route.recordId : null);
      setSelectedNotificationId(route.view === "notification-detail" ? route.recordId : null);
      setConnectorDrillTarget(
        route.view === "connectors" ? connectorTargetFromParams(route.params) : null
      );
    }

    window.addEventListener("hashchange", syncViewFromHash);

    return () => {
      window.removeEventListener("hashchange", syncViewFromHash);
    };
  }, []);

  useEffect(() => {
    const requestId = restoreRequestIdRef.current + 1;
    restoreRequestIdRef.current = requestId;

    async function restore() {
      if (!token) {
        setUser(null);
        setRestoreError(null);
        setBooting(false);
        return;
      }

      if (isSessionExpired(storedExpiresAt)) {
        setUser(null);
        setRestoreError(null);
        setBooting(false);
        setToken(null);
        return;
      }

      setBooting(true);
      setRestoreError(null);
      setUser(null);

      try {
        const me = await createApiClient(token, {
          onUnauthorized: handleUnauthorized
        }).get<MeResponse>("/me");
        if (restoreRequestIdRef.current !== requestId) {
          return;
        }

        const expiresAt = validSessionExpiry(me.expires_at) ? me.expires_at : storedExpiresAt;
        if (isSessionExpired(expiresAt)) {
          setUser(null);
          setToken(null);
          return;
        }

        setUser(me);
        if (me.expires_at && me.expires_at !== storedExpiresAt && validSessionExpiry(me.expires_at)) {
          setToken(token, me.expires_at);
        }
      } catch (error) {
        if (restoreRequestIdRef.current !== requestId) {
          return;
        }

        if (isApiError(error) && error.status === 401) {
          setUser(null);
          setToken(null);
          return;
        }

        setUser(null);
        setRestoreError(error instanceof Error ? error : new Error(String(error)));
      } finally {
        if (restoreRequestIdRef.current === requestId) {
          setBooting(false);
        }
      }
    }

    restore();
    return () => {
      if (restoreRequestIdRef.current === requestId) {
        restoreRequestIdRef.current += 1;
      }
    };
  }, [handleUnauthorized, restoreAttempt, setToken, token]);

  useEffect(() => {
    if (!token || !user) {
      return;
    }

    const params = microsoftOAuthCallbackParams();
    if (!params) {
      return;
    }
    const callbackKey = params.toString();
    if (!callbackKey || handledMicrosoftOAuthCallbackRef.current === callbackKey) {
      return;
    }
    handledMicrosoftOAuthCallbackRef.current = callbackKey;

    let mounted = true;
    async function finishMicrosoftOAuth() {
      try {
        const response = await client.post<MicrosoftOAuthCallbackResponse>(
          "/connectors/oauth/microsoft/callback",
          {
            code: params.get("code"),
            state: params.get("state") || "",
            redirect_uri: microsoftOAuthRedirectUri(),
            error: params.get("error"),
            error_description: params.get("error_description")
          }
        );
        if (!mounted) {
          return;
        }
        notifications.show({
          title: "Microsoft connected",
          message: response.source,
          color: "teal"
        });
        clearMicrosoftOAuthCallbackUrl();
        openConnectorDrill({ source: response.source });
      } catch (error) {
        if (!mounted) {
          return;
        }
        notifications.show({
          title: "Microsoft connection failed",
          message: error instanceof Error ? error.message : String(error),
          color: "red"
        });
        clearMicrosoftOAuthCallbackUrl();
        handleViewChange("connectors");
      }
    }

    finishMicrosoftOAuth();

    return () => {
      mounted = false;
    };
  }, [client, token, user]);

  async function handleLogin(credentials: LoginRequest) {
    const login = await createApiClient(null).post<LoginResponse>("/login", credentials);
    setBooting(true);
    setUser(null);
    setRestoreError(null);
    setToken(login.token, login.expires_at);
    notifications.show({
      title: "Signed in",
      message: "Session restored",
      color: "teal"
    });
  }

  async function handleLogout() {
    try {
      await client.post("/logout", {});
    } catch {
      // Session might already be expired.
    }
    setUser(null);
    setRestoreError(null);
    setToken(null);
  }

  function retrySessionRestore() {
    setRestoreError(null);
    setBooting(true);
    setRestoreAttempt((attempt) => attempt + 1);
  }

  function abandonStoredSession() {
    setUser(null);
    setRestoreError(null);
    setBooting(false);
    setToken(null);
  }

  function handleViewChange(nextView: AppView) {
    setView(nextView);
    setSelectedServiceId(null);
    setSelectedWorkCardId(null);
    setSelectedNotificationId(null);
    setConnectorDrillTarget(null);

    if (TOP_LEVEL_VIEWS.has(nextView)) {
      const nextHash = `#${nextView}`;
      if (window.location.hash !== nextHash) {
        window.location.hash = nextHash;
      }
    }
  }

  function openServiceOverview(serviceId: string | number) {
    setSelectedServiceId(Number(serviceId));
    setSelectedWorkCardId(null);
    setSelectedNotificationId(null);
    setConnectorDrillTarget(null);
    setView("service-overview");
  }

  function openConnectorDrill(target: ConnectorDrillTarget) {
    const nextHash = connectorHash(target);
    setSelectedServiceId(null);
    setSelectedWorkCardId(null);
    setSelectedNotificationId(null);
    setConnectorDrillTarget(target);
    setView("connectors");

    if (window.location.hash !== nextHash) {
      window.location.hash = nextHash;
    }
  }

  function openWorkCardDetail(workCardId: string | number) {
    const id = Number(workCardId);
    if (!Number.isInteger(id) || id <= 0) {
      return;
    }

    setSelectedServiceId(null);
    setSelectedWorkCardId(id);
    setSelectedNotificationId(null);
    setConnectorDrillTarget(null);
    setView("work-card-detail");

    const nextHash = `#work-cards/${id}`;
    if (window.location.hash !== nextHash) {
      window.location.hash = nextHash;
    }
  }

  function openNotificationDetail(notificationId: string | number) {
    const id = Number(notificationId);
    if (!Number.isInteger(id) || id <= 0) {
      return;
    }

    setSelectedServiceId(null);
    setSelectedWorkCardId(null);
    setSelectedNotificationId(id);
    setConnectorDrillTarget(null);
    setView("notification-detail");

    const nextHash = `#notifications/${id}`;
    if (window.location.hash !== nextHash) {
      window.location.hash = nextHash;
    }
  }

  if (booting) {
    return (
      <CenterStage>
        <PageLoader />
      </CenterStage>
    );
  }

  if (token && !user && restoreError) {
    return (
      <SessionRecoveryScreen
        error={restoreError}
        onRetry={retrySessionRestore}
        onSignOut={abandonStoredSession}
      />
    );
  }

  if (!token || !user) {
    return <LoginScreen onLogin={handleLogin} />;
  }

  if (!canAccessView(user, view)) {
    return (
      <PortalShell user={user} view={view} onLogout={handleLogout}>
        <AccessDenied onGoHome={() => handleViewChange("dashboard")} />
      </PortalShell>
    );
  }

  return (
    <PortalShell
      user={user}
      view={
        view === "service-overview" ||
        view === "work-card-detail" ||
        view === "notification-detail"
          ? "dashboard"
          : view
      }
      onLogout={handleLogout}
    >
      {view === "dashboard" && (
        <DashboardView
          client={client}
          canManageConnectors={user.capabilities.manage_connectors}
          onOpenService={openServiceOverview}
          onOpenConnector={openConnectorDrill}
          onOpenWorkCard={openWorkCardDetail}
          onOpenNotification={openNotificationDetail}
        />
      )}
      {view === "service-overview" && selectedServiceId && (
        <ServiceOverviewView
          client={client}
          serviceId={selectedServiceId}
          onBack={() => handleViewChange("dashboard")}
        />
      )}
      {view === "work-card-detail" && selectedWorkCardId && (
        <WorkCardDetailView
          client={client}
          workCardId={selectedWorkCardId}
          onBack={() => handleViewChange("dashboard")}
          onOpenConnector={openConnectorDrill}
        />
      )}
      {view === "notification-detail" && selectedNotificationId && (
        <NotificationDetailView
          client={client}
          notificationId={selectedNotificationId}
          onBack={() => handleViewChange("dashboard")}
          onOpenConnector={openConnectorDrill}
        />
      )}
      {view === "connectors" && (
        <ConnectorsView
          client={client}
          drillTarget={connectorDrillTarget}
          onOpenService={openServiceOverview}
        />
      )}
      {view === "catalog" && <CatalogView client={client} user={user} />}
      {view === "audit" && <AuditView client={client} />}
    </PortalShell>
  );
}

function validSessionExpiry(value?: string | null): value is string {
  return sessionExpiryMilliseconds(value) !== null;
}

function isSessionExpired(value?: string | null): boolean {
  const expiresAt = sessionExpiryMilliseconds(value);
  return expiresAt !== null && expiresAt <= Date.now();
}

function sessionExpiryMilliseconds(value?: string | null): number | null {
  if (!value) {
    return null;
  }

  // Rocket serializes UTC NaiveDateTime without a timezone suffix. Treat that wire format as UTC.
  const normalized = /(?:Z|[+-]\d{2}:?\d{2})$/i.test(value) ? value : `${value}Z`;
  const parsed = Date.parse(normalized);
  return Number.isFinite(parsed) ? parsed : null;
}
