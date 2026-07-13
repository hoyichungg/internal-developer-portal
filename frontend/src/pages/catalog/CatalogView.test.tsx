import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { createMockApiClient } from "../../test/mockApiClient";
import { renderWithProviders } from "../../test/render";
import type { MeResponse } from "../../types/api";
import { CatalogView } from "./CatalogView";

const maintainer = {
  id: 1,
  display_name: "Platform Team",
  email: "platform@example.test",
  created_at: "2026-05-19T00:00:00Z"
};

const service = {
  id: 7,
  maintainer_id: 1,
  slug: "identity-api",
  name: "Identity API",
  lifecycle_status: "active",
  health_status: "healthy",
  description: null,
  repository_url: null,
  dashboard_url: null,
  runbook_url: null,
  last_checked_at: null,
  created_at: "2026-05-19T00:00:00Z",
  updated_at: "2026-05-19T00:00:00Z",
  source: "monitoring",
  external_id: "identity-api"
};

const packageRecord = {
  id: 9,
  maintainer_id: 1,
  slug: "identity-sdk",
  name: "Identity SDK",
  version: "1.0.0",
  description: null,
  created_at: "2026-05-19T00:00:00Z",
  status: "active",
  repository_url: null,
  documentation_url: null,
  updated_at: "2026-05-19T00:00:00Z"
};

describe("CatalogView capability controls", () => {
  it("keeps a viewer read-only and does not request the user directory", async () => {
    const { client, calls } = createMockApiClient({
      "GET /maintainers": [maintainer],
      "GET /services": [service],
      "GET /packages": [packageRecord]
    });

    renderWithProviders(<CatalogView client={client} user={viewerUser()} />);

    expect(await screen.findByText("Identity API")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "New maintainer" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "New service" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "New package" })).toBeDisabled();
    expect(screen.queryByLabelText("Edit service Identity API")).not.toBeInTheDocument();
    expect(screen.queryByText("Maintainer members")).not.toBeInTheDocument();
    expect(calls.some((call) => call.path === "/users")).toBe(false);
  });

  it("shows administrative controls to an administrator", async () => {
    const { client, calls } = createMockApiClient({
      "GET /maintainers": [maintainer],
      "GET /services": [service],
      "GET /packages": [packageRecord],
      "GET /users": [
        { id: 1, username: "admin", roles: ["admin"], created_at: "2026-05-19T00:00:00Z" }
      ],
      "GET /maintainers/1/members": []
    });

    renderWithProviders(<CatalogView client={client} user={adminUser()} />);

    expect(await screen.findByRole("button", { name: "New maintainer" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New service" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "New package" })).toBeEnabled();
    expect(screen.getByLabelText("Edit service Identity API")).toBeInTheDocument();
    expect(screen.getByText("Maintainer members")).toBeInTheDocument();
    await waitFor(() =>
      expect(calls.some((call) => call.path === "/maintainers/1/members")).toBe(true)
    );
  });
});

function viewerUser(): MeResponse {
  return {
    id: 2,
    username: "viewer",
    roles: ["member"],
    auth_method: "password",
    capabilities: {
      manage_connectors: false,
      view_audit: false,
      manage_maintainers: false,
      view_user_directory: false
    },
    maintainer_access: [
      { maintainer_id: 1, role: "viewer", can_write: false, can_manage_members: false }
    ]
  };
}

function adminUser(): MeResponse {
  return {
    id: 1,
    username: "admin",
    roles: ["admin"],
    auth_method: "password",
    capabilities: {
      manage_connectors: true,
      view_audit: true,
      manage_maintainers: true,
      view_user_directory: true
    },
    maintainer_access: []
  };
}
