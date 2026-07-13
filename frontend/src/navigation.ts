import {
  IconBriefcase,
  IconHistory,
  IconLayoutDashboard,
  IconPlugConnected,
  IconTopologyStar
} from "@tabler/icons-react";

import type { MeCapabilities } from "./types/api";

type NavigationItem = {
  id: string;
  label: string;
  icon: typeof IconLayoutDashboard;
  capability?: keyof MeCapabilities;
};

export const NAV_ITEMS: NavigationItem[] = [
  { id: "dashboard", label: "Dashboard", icon: IconLayoutDashboard },
  { id: "my-work", label: "My Work", icon: IconBriefcase },
  {
    id: "connectors",
    label: "Connectors",
    icon: IconPlugConnected,
    capability: "manage_connectors"
  },
  { id: "catalog", label: "Catalog", icon: IconTopologyStar },
  { id: "audit", label: "Audit", icon: IconHistory, capability: "view_audit" }
];
