import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { createMockApiClient } from "../../test/mockApiClient";
import { renderWithProviders } from "../../test/render";
import { AuditView } from "./AuditView";

describe("AuditView layout", () => {
  it("keeps long audit fields inside compact table cells", async () => {
    const longAction = "connector_config_upsert_with_a_very_long_operational_action_name";
    const longMetadata = JSON.stringify({
      request_id: "req_1234567890abcdefghijklmnopqrstuvwxyz",
      connector_source: "microsoft-graph-calendar-production-primary-tenant",
      config_snapshot: {
        calendar_view_url:
          "https://graph.microsoft.com/v1.0/users/platform-team@example.test/calendarView"
      },
      error:
        "A long diagnostic message should stay inside the metadata preview instead of stretching the audit table"
    });
    const { client } = createMockApiClient({
      "GET /audit-logs": [
        {
          id: 1,
          actor_user_id: 7,
          action: longAction,
          resource_type: "connector_config",
          resource_id: "12345678901234567890",
          metadata: longMetadata,
          created_at: "2026-05-19T08:00:00+08:00"
        }
      ],
      "GET /users": [
        {
          id: 7,
          username: "platform-admin-with-a-long-name",
          roles: ["admin"],
          created_at: "2026-05-19T00:00:00Z"
        }
      ]
    });

    renderWithProviders(<AuditView client={client} />);

    const action = await screen.findByText(longAction.replaceAll("_", " "));
    const actor = await screen.findByText("platform-admin-with-a-long-name");
    const resource = await screen.findByText("connector_config");
    const resourceId = await screen.findByText("12345678901234567890");

    expect(action).toHaveClass("auditActionCell");
    expect(actor).toHaveClass("auditActorName");
    expect(resource).toHaveClass("auditCompactCell");
    expect(resourceId).toHaveClass("auditCompactCell");
    expect(screen.getByLabelText("View metadata")).toBeInTheDocument();

    await waitFor(() => expect(client.get).toHaveBeenCalledWith("/audit-logs"));
  });
});
