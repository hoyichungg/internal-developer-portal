import { describe, expect, it } from "vitest";

import type { ConnectorConfigForm } from "../../types/api";
import { connectorConfigDiagnostics, connectorConfigFromTemplate } from "./connectorConfig";

describe("connectorConfigDiagnostics", () => {
  it("flags invalid JSON before admins save config", () => {
    const diagnostics = connectorConfigDiagnostics(
      connectorConfigForm({
        config: "{",
        sample_payload: JSON.stringify({ items: [] })
      })
    );

    expect(diagnostics.map((diagnostic) => diagnostic.message)).toContain(
      "Config JSON must be valid JSON."
    );
  });

  it("flags adapter target mismatch and missing endpoint helpers", () => {
    const diagnostics = connectorConfigDiagnostics(
      connectorConfigForm({
        target: "work_cards",
        config: JSON.stringify({ adapter: "erp_private_messages" })
      })
    );
    const messages = diagnostics.map((diagnostic) => diagnostic.message);

    expect(messages).toContain("erp_private_messages config requires target notifications.");
    expect(messages).toContain("Set one of messages_url, private_messages_url, url.");
  });

  it("flags malformed adapter declarations", () => {
    const diagnostics = connectorConfigDiagnostics(
      connectorConfigForm({
        config: JSON.stringify({ adapter: 123 })
      })
    );

    expect(diagnostics.map((diagnostic) => diagnostic.message)).toContain(
      "adapter must be a string."
    );
  });

  it("flags adapter-specific URL and range issues", () => {
    const diagnostics = connectorConfigDiagnostics(
      connectorConfigForm({
        target: "service_health",
        config: JSON.stringify({
          adapter: "monitoring",
          url: "ftp://monitoring.example.test/feed",
          timeout_seconds: 0
        })
      })
    );
    const messages = diagnostics.map((diagnostic) => diagnostic.message);

    expect(messages).toContain("url must be an absolute HTTP URL.");
    expect(messages).toContain("timeout_seconds must be a positive integer.");
  });

  it("accepts the ERP private messages template", () => {
    const config = connectorConfigFromTemplate("erp_private_messages");

    expect(config).not.toBeNull();
    expect(connectorConfigDiagnostics(config as ConnectorConfigForm)).toEqual([]);
  });

  it("requires ERP snapshot reconciliation to be an explicit boolean", () => {
    const messages = connectorConfigDiagnostics(
      connectorConfigForm({
        config: JSON.stringify({
          adapter: "erp_private_messages",
          messages_url: "https://erp.example.test/messages",
          snapshot_complete: "yes"
        })
      })
    ).map((diagnostic) => diagnostic.message);

    expect(messages).toContain("snapshot_complete must be a boolean.");
  });

  it("validates Graph and Azure collection safety limits", () => {
    const graphMessages = connectorConfigDiagnostics(
      connectorConfigForm({
        config: JSON.stringify({
          adapter: "microsoft_graph_mail",
          max_pages: 0,
          max_items: 10001
        })
      })
    ).map((diagnostic) => diagnostic.message);
    const azureMessages = connectorConfigDiagnostics(
      connectorConfigForm({
        target: "work_cards",
        config: JSON.stringify({
          adapter: "azure_devops",
          organization: "acme",
          project: "portal",
          max_items: -1
        })
      })
    ).map((diagnostic) => diagnostic.message);

    expect(graphMessages).toContain("max_pages must be an integer from 1 to 100.");
    expect(graphMessages).toContain("max_items must be an integer from 1 to 10000.");
    expect(azureMessages).toContain("max_items must be an integer from 1 to 10000.");
  });

  it("validates Azure My Work identity mapping and due-date configuration", () => {
    const messages = connectorConfigDiagnostics(
      connectorConfigForm({
        target: "work_cards",
        config: JSON.stringify({
          adapter: "azure_devops",
          organization: "acme",
          project: "portal",
          due_date_field: " ",
          assignee_user_mappings: {
            "": 1,
            "aad.invalid": 0
          }
        })
      })
    ).map((diagnostic) => diagnostic.message);

    expect(messages).toContain("due_date_field must be a non-empty string when provided.");
    expect(messages).toContain(
      "assignee_user_mappings keys must be non-empty source descriptors of at most 512 characters."
    );
    expect(messages).toContain(
      "assignee_user_mappings values must be positive portal user ids."
    );
  });

  it("rejects ambiguous connector datetimes before they are saved", () => {
    const messages = connectorConfigDiagnostics(
      connectorConfigForm({
        target: "calendar_events",
        config: JSON.stringify({
          adapter: "calendar_sample",
          events: [
            {
              id: "standup",
              starts_at: "2026-07-10T09:00:00",
              ends_at: "2026-07-10T09:30:00"
            }
          ]
        }),
        sample_payload: JSON.stringify({
          items: [
            {
              external_id: "standup",
              starts_at: "2026-07-10T09:00:00",
              ends_at: "2026-07-10T09:30:00"
            }
          ]
        })
      })
    ).map((diagnostic) => diagnostic.message);

    expect(messages).toContain(
      "Config JSON events[0].starts_at must be RFC3339 with Z or an explicit offset."
    );
    expect(messages).toContain(
      "Sample payload items[0].ends_at must be RFC3339 with Z or an explicit offset."
    );
  });

  it("accepts explicit non-UTC offsets and requires both calendar bounds", () => {
    const valid = connectorConfigDiagnostics(
      connectorConfigForm({
        target: "calendar_events",
        config: JSON.stringify({ adapter: "microsoft_graph_calendar" }),
        sample_payload: JSON.stringify({
          items: [
            {
              starts_at: "2026-07-10T09:00:00+08:00",
              ends_at: "2026-07-10T09:30:00+08:00"
            }
          ]
        })
      })
    );
    const missingEnd = connectorConfigDiagnostics(
      connectorConfigForm({
        target: "calendar_events",
        sample_payload: JSON.stringify({
          items: [{ starts_at: "2026-07-10T09:00:00Z" }]
        })
      })
    ).map((diagnostic) => diagnostic.message);

    expect(valid).toEqual([]);
    expect(missingEnd).toContain(
      "Sample payload items[0].ends_at is required and must be RFC3339 with Z or an explicit offset."
    );
  });

  it("ships only offset-aware datetime examples in calendar templates", () => {
    for (const templateId of ["microsoft_graph_calendar", "calendar_notifications"]) {
      const template = connectorConfigFromTemplate(templateId) as ConnectorConfigForm;
      const config = JSON.parse(template.config) as {
        events?: Array<{ starts_at?: string; ends_at?: string }>;
      };
      const payload = JSON.parse(template.sample_payload) as {
        items: Array<{ starts_at: string; ends_at: string }>;
      };

      for (const item of [...(config.events || []), ...payload.items]) {
        expect(item.starts_at).toMatch(/(?:Z|[+-]\d{2}:\d{2})$/);
        expect(item.ends_at).toMatch(/(?:Z|[+-]\d{2}:\d{2})$/);
      }
    }
  });

  it.each([
    "monitoring_service_health",
    "azure_devops_work_cards",
    "microsoft_graph_calendar",
    "calendar_notifications",
    "outlook_mail_notifications"
  ])("ships a valid bounded connector template: %s", (templateId) => {
    const config = connectorConfigFromTemplate(templateId);

    expect(config).not.toBeNull();
    expect(connectorConfigDiagnostics(config as ConnectorConfigForm)).toEqual([]);
  });
});

function connectorConfigForm(overrides: Partial<ConnectorConfigForm>): ConnectorConfigForm {
  return {
    target: "notifications",
    enabled: true,
    schedule_cron: "@every 15m",
    config: JSON.stringify({ adapter: "microsoft_graph_calendar" }),
    sample_payload: JSON.stringify({ items: [] }),
    ...overrides
  };
}
