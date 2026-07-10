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

  it.each([
    "azure_devops_work_cards",
    "microsoft_graph_calendar",
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
