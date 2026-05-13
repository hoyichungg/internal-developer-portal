import { notifications } from "@mantine/notifications";
import { useEffect, useMemo, useRef, useState } from "react";

import { createApiClient } from "./api/client";
import { CenterStage } from "./components/CenterStage";
import { PageLoader } from "./components/LoadingState";
import { useStoredToken } from "./hooks/useStoredToken";
import { PortalShell } from "./layout/PortalShell";
import { AuditView } from "./pages/audit/AuditView";
import { CatalogView } from "./pages/catalog/CatalogView";
import { ConnectorsView } from "./pages/connectors/ConnectorsView";
import { DashboardView } from "./pages/dashboard/DashboardView";
import { LoginScreen } from "./pages/login/LoginScreen";
import { ServiceOverviewView } from "./pages/services/ServiceOverviewView";
import type {
  ApiId,
  ConnectorDrillTarget,
  LoginRequest,
  LoginResponse,
  MeResponse
} from "./types/api";

const TOP_LEVEL_VIEWS = new Set(["dashboard", "connectors", "catalog", "audit"]);
type AppView = "dashboard" | "connectors" | "catalog" | "audit" | "service-overview";

function viewFromHash(): AppView {
  return routeFromHash().view;
}

function routeFromHash(): { view: AppView; params: URLSearchParams } {
  const hash = window.location.hash.replace(/^#\/?/, "");
  const [viewPart, query = ""] = hash.split("?");
  const view = TOP_LEVEL_VIEWS.has(viewPart) ? (viewPart as AppView) : "dashboard";

  return { view, params: new URLSearchParams(query) };
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

export default function App() {
  const [token, setToken] = useStoredToken();
  const [user, setUser] = useState<MeResponse | null>(null);
  const [view, setView] = useState(viewFromHash);
  const [selectedServiceId, setSelectedServiceId] = useState<ApiId | null>(null);
  const [connectorDrillTarget, setConnectorDrillTarget] =
    useState<ConnectorDrillTarget | null>(() => {
      const route = routeFromHash();
      return route.view === "connectors" ? connectorTargetFromParams(route.params) : null;
    });
  const [booting, setBooting] = useState(true);
  const restoredTokenRef = useRef<string | null>(null);

  const client = useMemo(() => createApiClient(token), [token]);

  useEffect(() => {
    function syncViewFromHash() {
      const route = routeFromHash();
      setView(route.view);
      setSelectedServiceId(null);
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
    let mounted = true;

    async function restore() {
      if (!token) {
        restoredTokenRef.current = null;
        setUser(null);
        setBooting(false);
        return;
      }

      if (restoredTokenRef.current === token) {
        setBooting(false);
        return;
      }

      restoredTokenRef.current = token;
      setBooting(true);

      try {
        const me = await createApiClient(token).get<MeResponse>("/me");
        if (mounted) {
          setUser(me);
        }
      } catch {
        if (mounted) {
          restoredTokenRef.current = null;
          setUser(null);
          setToken(null);
        }
      } finally {
        if (mounted) {
          setBooting(false);
        }
      }
    }

    restore();
    return () => {
      mounted = false;
    };
  }, [setToken, token]);

  async function handleLogin(credentials: LoginRequest) {
    const login = await createApiClient(null).post<LoginResponse>("/login", credentials);
    setBooting(true);
    setUser(null);
    setSelectedServiceId(null);
    setConnectorDrillTarget(null);
    setToken(login.token);
    handleViewChange("dashboard");
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
    setSelectedServiceId(null);
    setConnectorDrillTarget(null);
    setToken(null);
  }

  function handleViewChange(nextView: AppView) {
    setView(nextView);
    setSelectedServiceId(null);
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
    setConnectorDrillTarget(null);
    setView("service-overview");
  }

  function openConnectorDrill(target: ConnectorDrillTarget) {
    const nextHash = connectorHash(target);
    setSelectedServiceId(null);
    setConnectorDrillTarget(target);
    setView("connectors");

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

  if (!token || !user) {
    return <LoginScreen onLogin={handleLogin} />;
  }

  return (
    <PortalShell
      user={user}
      view={view === "service-overview" ? "dashboard" : view}
      onLogout={handleLogout}
    >
      {view === "dashboard" && (
        <DashboardView
          client={client}
          onOpenService={openServiceOverview}
          onOpenConnector={openConnectorDrill}
        />
      )}
      {view === "service-overview" && selectedServiceId && (
        <ServiceOverviewView
          client={client}
          serviceId={selectedServiceId}
          onBack={() => handleViewChange("dashboard")}
        />
      )}
      {view === "connectors" && (
        <ConnectorsView
          client={client}
          drillTarget={connectorDrillTarget}
          onOpenService={openServiceOverview}
        />
      )}
      {view === "catalog" && <CatalogView client={client} />}
      {view === "audit" && <AuditView client={client} />}
    </PortalShell>
  );
}
