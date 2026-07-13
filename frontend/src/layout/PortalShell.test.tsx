import { screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders } from "../test/render";
import type { MeResponse } from "../types/api";
import { PortalShell } from "./PortalShell";

describe("PortalShell capability navigation", () => {
  it("hides administrative navigation from a regular member", () => {
    renderWithProviders(
      <PortalShell
        user={memberUser()}
        view="dashboard"
        onLogout={vi.fn()}
        onRevokeAllSessions={vi.fn()}
      >
        <div>Dashboard content</div>
      </PortalShell>
    );

    expect(screen.getByRole("link", { name: "Dashboard" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "My Work" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Catalog" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Connectors" })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Audit" })).not.toBeInTheDocument();
  });

  it("shows administrative navigation to an administrator", () => {
    const user = memberUser();
    user.roles = ["admin"];
    user.capabilities.manage_connectors = true;
    user.capabilities.view_audit = true;

    renderWithProviders(
      <PortalShell
        user={user}
        view="dashboard"
        onLogout={vi.fn()}
        onRevokeAllSessions={vi.fn()}
      >
        <div>Dashboard content</div>
      </PortalShell>
    );

    expect(screen.getByRole("link", { name: "Connectors" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Audit" })).toBeInTheDocument();
  });
});

function memberUser(): MeResponse {
  return {
    id: 2,
    username: "member",
    roles: ["member"],
    auth_method: "password",
    capabilities: {
      manage_connectors: false,
      view_audit: false,
      manage_maintainers: false,
      view_user_directory: false
    },
    maintainer_access: []
  };
}
