import { notifications } from "@mantine/notifications";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { createApiClient, isApiError } from "./api/client";
import { AccessDenied } from "./components/AccessDenied";
import { CenterStage } from "./components/CenterStage";
import { PageLoader } from "./components/LoadingState";
import { SessionRecoveryScreen } from "./components/SessionRecoveryScreen";
import { PortalShell } from "./layout/PortalShell";
import { AuditView } from "./pages/audit/AuditView";
import { CatalogView } from "./pages/catalog/CatalogView";
import { ConnectorsView } from "./pages/connectors/ConnectorsView";
import { DashboardView } from "./pages/dashboard/DashboardView";
import {
  entraLoginStartUrl,
  hasAuthCallbackParameters,
  parseEntraAuthCallback,
  urlWithoutAuthCallbackParameters
} from "./pages/login/authRouting";
import { LoginScreen } from "./pages/login/LoginScreen";
import { NotificationDetailView } from "./pages/records/NotificationDetailView";
import { WorkCardDetailView } from "./pages/records/WorkCardDetailView";
import { ServiceOverviewView } from "./pages/services/ServiceOverviewView";
import { MyWorkView } from "./pages/work/MyWorkView";
import { myWorkReturnHash } from "./pages/work/myWorkRouting";
import {
  clearLegacySessionCredentials,
  publishSessionEvent,
  subscribeToSessionEvents
} from "./sessionEvents";
import type {
  ApiId,
  ConnectorDrillTarget,
  LoginRequest,
  LoginResponse,
  MeResponse,
  MicrosoftOAuthCallbackResponse,
  PublicAuthConfig,
  RevokeAllSessionsResponse
} from "./types/api";

const TOP_LEVEL_VIEWS = new Set(["dashboard", "my-work", "connectors", "catalog", "audit"]);
const UNCONFIRMED_ENTRA_SIGN_IN = "Microsoft sign-in could not be confirmed. Try again.";
const UNCONFIRMED_PASSWORD_SIGN_IN = "Password sign-in could not be confirmed. Try again.";
type AppView =
  | "dashboard"
  | "my-work"
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

function connectorMicrosoftOAuthCallbackParams(): URLSearchParams | null {
  if (window.location.pathname !== "/oauth/microsoft/callback") {
    return null;
  }

  return new URLSearchParams(window.location.search);
}

function connectorMicrosoftOAuthRedirectUri(): string {
  return `${window.location.origin}/oauth/microsoft/callback`;
}

function clearConnectorMicrosoftOAuthCallbackUrl() {
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
  const initialEntraAuthCallback = useMemo(
    () =>
      window.location.pathname === "/" ? parseEntraAuthCallback(window.location.search) : null,
    []
  );
  const [user, setUser] = useState<MeResponse | null>(null);
  const [authConfig, setAuthConfig] = useState<PublicAuthConfig | null>(null);
  const [authConfigError, setAuthConfigError] = useState<Error | null>(null);
  const [authConfigAttempt, setAuthConfigAttempt] = useState(0);
  const [authCallbackError, setAuthCallbackError] = useState<string | null>(() =>
    initialEntraAuthCallback?.kind === "error" ? initialEntraAuthCallback.message : null
  );
  const [view, setView] = useState(() => initialRoute.view);
  const [selectedServiceId, setSelectedServiceId] = useState<ApiId | null>(null);
  const [selectedWorkCardId, setSelectedWorkCardId] = useState<ApiId | null>(() =>
    initialRoute.view === "work-card-detail" ? initialRoute.recordId : null
  );
  const [selectedWorkCardBackHash, setSelectedWorkCardBackHash] = useState(() =>
    initialRoute.view === "work-card-detail"
      ? myWorkReturnHash(initialRoute.params) || "#dashboard"
      : "#dashboard"
  );
  const [myWorkSearchParams, setMyWorkSearchParams] = useState(
    () => new URLSearchParams(initialRoute.view === "my-work" ? initialRoute.params : undefined)
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
  const sessionGenerationRef = useRef(0);
  const handledConnectorMicrosoftOAuthCallbackRef = useRef("");
  const handledEntraAuthCallbackRef = useRef(false);

  const clearSessionState = useCallback(() => {
    sessionGenerationRef.current += 1;
    restoreRequestIdRef.current += 1;
    setUser(null);
    setRestoreError(null);
    setBooting(false);
  }, []);

  const requestSessionRestore = useCallback(() => {
    restoreRequestIdRef.current += 1;
    setUser(null);
    setRestoreError(null);
    setBooting(true);
    setRestoreAttempt((attempt) => attempt + 1);
  }, []);

  const handleUnauthorized = useCallback(() => {
    clearSessionState();
    publishSessionEvent("signed-out");
  }, [clearSessionState]);

  const client = useMemo(
    () =>
      createApiClient({
        onUnauthorized: handleUnauthorized,
        getSessionGeneration: () => sessionGenerationRef.current
      }),
    [handleUnauthorized]
  );
  const sessionProbeClient = useMemo(() => createApiClient(), []);

  useEffect(() => {
    function syncViewFromHash() {
      const route = routeFromHash();
      setView(route.view);
      setSelectedServiceId(null);
      setSelectedWorkCardId(route.view === "work-card-detail" ? route.recordId : null);
      setSelectedWorkCardBackHash(
        route.view === "work-card-detail"
          ? myWorkReturnHash(route.params) || "#dashboard"
          : "#dashboard"
      );
      setMyWorkSearchParams(
        new URLSearchParams(route.view === "my-work" ? route.params : undefined)
      );
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
    clearLegacySessionCredentials();
  }, []);

  useLayoutEffect(() => {
    if (
      window.location.pathname !== "/" ||
      !hasAuthCallbackParameters(window.location.search)
    ) {
      return;
    }

    window.history.replaceState(
      null,
      "",
      urlWithoutAuthCallbackParameters(window.location.href)
    );
  }, []);

  useEffect(
    () =>
      subscribeToSessionEvents((event) => {
        if (event === "signed-out") {
          clearSessionState();
          return;
        }

        sessionGenerationRef.current += 1;
        requestSessionRestore();
      }),
    [clearSessionState, requestSessionRestore]
  );

  useEffect(() => {
    const requestId = restoreRequestIdRef.current + 1;
    restoreRequestIdRef.current = requestId;

    async function restore() {
      setBooting(true);
      setRestoreError(null);
      setUser(null);

      try {
        const me = await sessionProbeClient.get<MeResponse>("/me");
        if (restoreRequestIdRef.current !== requestId) {
          return;
        }

        setUser(me);
        setAuthCallbackError(null);
      } catch (error) {
        if (restoreRequestIdRef.current !== requestId) {
          return;
        }

        if (isApiError(error) && error.status === 401) {
          clearSessionState();
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
  }, [clearSessionState, restoreAttempt, sessionProbeClient]);

  useEffect(() => {
    if (user || booting || restoreError || authConfig || authConfigError) {
      return;
    }

    let active = true;
    setAuthConfigError(null);

    sessionProbeClient
      .get<PublicAuthConfig>("/auth/config")
      .then((config) => {
        if (active) {
          setAuthConfig(config);
        }
      })
      .catch((error) => {
        if (active) {
          setAuthConfigError(error instanceof Error ? error : new Error(String(error)));
        }
      });

    return () => {
      active = false;
    };
  }, [
    authConfig,
    authConfigAttempt,
    authConfigError,
    booting,
    restoreError,
    sessionProbeClient,
    user
  ]);

  useEffect(() => {
    if (
      !initialEntraAuthCallback ||
      handledEntraAuthCallbackRef.current ||
      booting ||
      restoreError
    ) {
      return;
    }

    handledEntraAuthCallbackRef.current = true;

    if (initialEntraAuthCallback.kind === "success") {
      if (user?.auth_method === "entra") {
        publishSessionEvent("signed-in");
        notifications.show({
          title: "Signed in with Microsoft",
          message: user.username,
          color: "teal"
        });
      } else {
        if (user) {
          notifications.show({
            title: "Microsoft sign-in failed",
            message: UNCONFIRMED_ENTRA_SIGN_IN,
            color: "red"
          });
        } else {
          setAuthCallbackError(UNCONFIRMED_ENTRA_SIGN_IN);
        }
      }
      return;
    }

    if (user) {
      setAuthCallbackError(null);
      notifications.show({
        title: "Microsoft sign-in failed",
        message: initialEntraAuthCallback.message,
        color: "red"
      });
    }
  }, [booting, initialEntraAuthCallback, restoreError, user]);

  useEffect(() => {
    if (!user) {
      return;
    }

    const params = connectorMicrosoftOAuthCallbackParams();
    if (!params) {
      return;
    }
    const callbackKey = params.toString();
    if (!callbackKey || handledConnectorMicrosoftOAuthCallbackRef.current === callbackKey) {
      return;
    }
    handledConnectorMicrosoftOAuthCallbackRef.current = callbackKey;
    // Keep the values only in this closure. Remove code/state from browser
    // history before the token-exchange request starts so later same-origin
    // navigation cannot disclose them through a Referer.
    clearConnectorMicrosoftOAuthCallbackUrl();

    let mounted = true;
    async function finishMicrosoftOAuth() {
      try {
        const response = await client.post<MicrosoftOAuthCallbackResponse>(
          "/connectors/oauth/microsoft/callback",
          {
            code: params.get("code"),
            state: params.get("state") || "",
            redirect_uri: connectorMicrosoftOAuthRedirectUri(),
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
        clearConnectorMicrosoftOAuthCallbackUrl();
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
        clearConnectorMicrosoftOAuthCallbackUrl();
        handleViewChange("connectors");
      }
    }

    finishMicrosoftOAuth();

    return () => {
      mounted = false;
    };
  }, [client, user]);

  async function handleLogin(credentials: LoginRequest) {
    if (authConfig?.password_login_enabled !== true) {
      throw new Error("Password sign-in is disabled.");
    }

    await createApiClient().post<LoginResponse>("/login", credentials);
    const me = await sessionProbeClient.get<MeResponse>("/me");
    if (me.auth_method !== "password") {
      throw new Error(UNCONFIRMED_PASSWORD_SIGN_IN);
    }

    sessionGenerationRef.current += 1;
    restoreRequestIdRef.current += 1;
    setUser(me);
    setRestoreError(null);
    setAuthCallbackError(null);
    setBooting(false);
    publishSessionEvent("signed-in");
    notifications.show({
      title: "Signed in",
      message: "Session established",
      color: "teal"
    });
  }

  async function handleLogout() {
    try {
      await client.post("/logout", {});
    } catch (error) {
      if (isApiError(error) && error.status === 401) {
        return;
      }

      notifications.show({
        title: "Sign out failed",
        message: error instanceof Error ? error.message : String(error),
        color: "red"
      });
      return;
    }
    clearSessionState();
    publishSessionEvent("signed-out");
  }

  async function handleRevokeAllSessions() {
    if (!window.confirm("Sign out this account on every device and browser?")) {
      return;
    }

    try {
      const result = await client.post<RevokeAllSessionsResponse>("/sessions/revoke-all", {});
      clearSessionState();
      publishSessionEvent("signed-out");
      notifications.show({
        title: "Signed out everywhere",
        message: `${result.revoked_sessions} session${result.revoked_sessions === 1 ? "" : "s"} revoked`,
        color: "teal"
      });
    } catch (error) {
      if (isApiError(error) && error.status === 401) {
        return;
      }

      notifications.show({
        title: "Could not revoke sessions",
        message: error instanceof Error ? error.message : String(error),
        color: "red"
      });
    }
  }

  function retrySessionRestore() {
    requestSessionRestore();
  }

  function retryAuthConfig() {
    setAuthConfig(null);
    setAuthConfigError(null);
    setAuthConfigAttempt((attempt) => attempt + 1);
  }

  function handleViewChange(nextView: AppView) {
    setView(nextView);
    setSelectedServiceId(null);
    setSelectedWorkCardId(null);
    setSelectedWorkCardBackHash("#dashboard");
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

  function openWorkCardDetail(workCardId: string | number, detailHash?: string) {
    const id = Number(workCardId);
    if (!Number.isInteger(id) || id <= 0) {
      return;
    }

    setSelectedServiceId(null);
    setSelectedWorkCardId(id);
    const detailParams = new URLSearchParams(detailHash?.split("?", 2)[1] || "");
    setSelectedWorkCardBackHash(myWorkReturnHash(detailParams) || "#dashboard");
    setSelectedNotificationId(null);
    setConnectorDrillTarget(null);
    setView("work-card-detail");

    const nextHash = detailHash || `#work-cards/${id}`;
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

  if (!user && restoreError) {
    return (
      <SessionRecoveryScreen
        error={restoreError}
        onRetry={retrySessionRestore}
        onSignOut={() => void handleLogout()}
      />
    );
  }

  if (!user) {
    return (
      <LoginScreen
        authConfig={authConfig}
        authConfigError={authConfigError}
        callbackError={authCallbackError}
        entraLoginUrl={entraLoginStartUrl(window.location.hash)}
        onLogin={handleLogin}
        onRetryAuthConfig={retryAuthConfig}
      />
    );
  }

  if (!canAccessView(user, view)) {
    return (
      <PortalShell
        user={user}
        view={view}
        onLogout={handleLogout}
        onRevokeAllSessions={handleRevokeAllSessions}
      >
        <AccessDenied onGoHome={() => handleViewChange("dashboard")} />
      </PortalShell>
    );
  }

  return (
    <PortalShell
      user={user}
      view={
        view === "service-overview" ||
        view === "notification-detail"
          ? "dashboard"
          : view === "work-card-detail"
            ? selectedWorkCardBackHash.startsWith("#my-work")
              ? "my-work"
              : "dashboard"
          : view
      }
      onLogout={handleLogout}
      onRevokeAllSessions={handleRevokeAllSessions}
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
      {view === "my-work" && (
        <MyWorkView
          client={client}
          searchParams={myWorkSearchParams}
          onNavigate={(hash) => {
            if (window.location.hash !== hash) {
              window.location.hash = hash;
            }
          }}
          onOpenWorkCard={(workCardId, detailHash) =>
            openWorkCardDetail(workCardId, detailHash)
          }
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
          onBack={() => {
            if (window.location.hash !== selectedWorkCardBackHash) {
              window.location.hash = selectedWorkCardBackHash;
            }
          }}
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
