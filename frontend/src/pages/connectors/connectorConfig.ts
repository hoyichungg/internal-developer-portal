import { prettyJson } from "../../utils/format";
import type { ConnectorConfigForm, ConnectorConfigResponse, JsonValue } from "../../types/api";

type ConnectorTemplate = {
  id: string;
  label: string;
  target: string;
  schedule_cron: string;
  config: JsonValue;
  sample_payload: JsonValue;
};

export type ConnectorConfigDiagnostic = {
  level: "error" | "warning";
  message: string;
};

type JsonRecord = Record<string, unknown>;

export type ConnectorConfigLoadState = "idle" | "loading" | "ready" | "missing" | "error";

export const defaultConnectorConfig: ConnectorConfigForm = {
  target: "work_cards",
  enabled: true,
  schedule_cron: "",
  config: JSON.stringify({ adapter: "azure_devops", max_items: 1000 }, null, 2),
  sample_payload: JSON.stringify({ items: [] }, null, 2)
};

export const connectorTemplates: ConnectorTemplate[] = [
  {
    id: "monitoring_service_health",
    label: "Monitoring service health",
    target: "service_health",
    schedule_cron: "@every 5m",
    config: {
      adapter: "monitoring",
      url: "https://monitoring.example.test/api/service-health",
      default_maintainer_id: 1,
      timeout_seconds: 15
    },
    sample_payload: {
      items: [
        {
          external_id: "identity-api",
          maintainer_id: 1,
          slug: "identity-api",
          name: "Identity API",
          lifecycle_status: "active",
          health_status: "degraded",
          description: "Authentication and session service",
          repository_url: "https://github.com/acme/identity-api",
          dashboard_url: "https://grafana.example.test/d/identity",
          runbook_url: "https://docs.example.test/runbooks/identity",
          last_checked_at: null
        }
      ]
    }
  },
  {
    id: "azure_devops_work_cards",
    label: "Azure DevOps work cards",
    target: "work_cards",
    schedule_cron: "@every 15m",
    config: {
      adapter: "azure_devops",
      organization: "acme",
      project: "platform",
      wiql: "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project ORDER BY [System.ChangedDate] DESC",
      web_url_base: "https://dev.azure.com/acme/platform/_workitems/edit",
      max_items: 1000,
      timeout_seconds: 15
    },
    sample_payload: {
      items: [
        {
          external_id: "ADO-42",
          title: "Review deployment pipeline",
          status: "in_progress",
          priority: "high",
          assignee: "platform-team",
          due_at: null,
          url: "https://dev.azure.com/acme/platform/_workitems/edit/42"
        }
      ]
    }
  },
  {
    id: "microsoft_graph_calendar",
    label: "Microsoft Graph calendar",
    target: "calendar_events",
    schedule_cron: "@every 15m",
    config: {
      adapter: "microsoft_graph_calendar",
      user_id: "me",
      tenant_id: "organizations",
      client_id: "",
      client_secret: "",
      refresh_token: "",
      scope: "https://graph.microsoft.com/Calendars.Read offline_access",
      time_zone: "UTC",
      lookahead_hours: 24,
      top: 25,
      max_pages: 20,
      max_items: 1000,
      timeout_seconds: 15
    },
    sample_payload: {
      items: [
        {
          external_id: "evt-standup",
          title: "Calendar: Platform standup",
          body: "Organizer: Taylor Lin | Location: Teams | Starts: 2026-05-19T09:30:00 UTC",
          organizer: "Taylor Lin",
          location: "Teams",
          starts_at: "2026-05-19T09:30:00",
          ends_at: "2026-05-19T10:00:00",
          time_zone: "UTC",
          is_all_day: false,
          is_cancelled: false,
          web_url: "https://outlook.office.com/calendar/item/evt-standup",
          join_url: "https://teams.microsoft.com/l/meetup-join/evt-standup"
        }
      ],
      snapshot_complete: true
    }
  },
  {
    id: "calendar_notifications",
    label: "Sample calendar events",
    target: "calendar_events",
    schedule_cron: "@every 30m",
    config: {
      adapter: "calendar_sample",
      events: [
        {
          id: "calendar-platform-standup",
          subject: "Calendar: Platform standup in 15 minutes",
          organizer: "Taylor Lin",
          location: "Teams",
          starts_at: "2026-05-11T09:30:00Z",
          ends_at: "2026-05-11T10:00:00Z",
          web_link: "https://calendar.example.test/events/platform-standup"
        }
      ]
    },
    sample_payload: {
      items: [
        {
          external_id: "calendar-platform-standup",
          title: "Calendar: Platform standup in 15 minutes",
          body: "Organizer: Taylor Lin | Location: Teams",
          organizer: "Taylor Lin",
          location: "Teams",
          starts_at: "2026-05-11T09:30:00",
          ends_at: "2026-05-11T10:00:00",
          time_zone: "UTC",
          is_all_day: false,
          is_cancelled: false,
          web_url: "https://calendar.example.test/events/platform-standup",
          join_url: null
        }
      ],
      snapshot_complete: true
    }
  },
  {
    id: "outlook_mail_notifications",
    label: "Outlook mail notifications",
    target: "notifications",
    schedule_cron: "@every 15m",
    config: {
      adapter: "microsoft_graph_mail",
      user_id: "me",
      mail_folder_id: "Inbox",
      tenant_id: "organizations",
      client_id: "",
      client_secret: "",
      refresh_token: "",
      scope: "https://graph.microsoft.com/Mail.Read offline_access",
      unread_only: true,
      lookback_hours: 24,
      top: 25,
      max_pages: 20,
      max_items: 1000,
      timeout_seconds: 15
    },
    sample_payload: {
      items: [
        {
          external_id: "release-brief",
          title: "Mail: Release brief ready for review",
          body: "From: release-bot@example.test | API deploy window moved to 15:30.",
          severity: "warning",
          is_read: false,
          url: "https://outlook.example.test/mail/release-brief"
        }
      ]
    }
  },
  {
    id: "erp_private_messages",
    label: "ERP private messages",
    target: "notifications",
    schedule_cron: "@every 15m",
    config: {
      adapter: "erp_private_messages",
      messages_url: "https://erp.example.test/api/private-messages",
      bearer_token: "",
      api_key: "",
      api_key_header: "x-api-key",
      unread_only: true,
      lookback_hours: 24,
      top: 25,
      snapshot_complete: false,
      timeout_seconds: 15
    },
    sample_payload: {
      items: [
        {
          external_id: "access-approval",
          title: "ERP: Deployment access approval waiting",
          body: "Production deployment access needs review.",
          severity: "warning",
          is_read: false,
          url: "https://erp.example.test/messages/access-approval"
        }
      ]
    }
  },
  {
    id: "erp_message_notifications",
    label: "ERP sample messages",
    target: "notifications",
    schedule_cron: "@every 15m",
    config: {
      adapter: "erp_messages_sample",
      messages: [
        {
          id: "access-approval",
          title: "ERP: Deployment access approval waiting",
          message: "Sample ERP private message. Use the ERP private messages template for a real HTTP endpoint.",
          requires_approval: true
        }
      ]
    },
    sample_payload: {
      items: [
        {
          external_id: "access-approval",
          title: "ERP: Deployment access approval waiting",
          body: "Sample ERP private message. Use the ERP private messages template for a real HTTP endpoint.",
          severity: "warning",
          is_read: false,
          url: null
        }
      ]
    }
  }
];

export function connectorConfigFromResponse(
  response: ConnectorConfigResponse | null
): ConnectorConfigForm {
  if (!response) {
    return defaultConnectorConfig;
  }

  return {
    target: response.target,
    enabled: response.enabled,
    schedule_cron: response.schedule_cron || "",
    config: prettyJson(response.config),
    sample_payload: prettyJson(response.sample_payload)
  };
}

export function connectorConfigFromTemplate(templateId: string): ConnectorConfigForm | null {
  const template = connectorTemplates.find((item) => item.id === templateId);

  if (!template) {
    return null;
  }

  return {
    target: template.target,
    enabled: true,
    schedule_cron: template.schedule_cron,
    config: JSON.stringify(template.config, null, 2),
    sample_payload: JSON.stringify(template.sample_payload, null, 2)
  };
}

export function connectorConfigDiagnostics(
  config: ConnectorConfigForm
): ConnectorConfigDiagnostic[] {
  const diagnostics: ConnectorConfigDiagnostic[] = [];
  const parsedConfig = parseJsonObject(config.config, "Config JSON", diagnostics);

  if (parsedConfig) {
    validateAdapterConfig(diagnostics, config.target, parsedConfig);
  }

  const parsedSamplePayload = parseJsonValue(config.sample_payload, "Sample payload", diagnostics);
  if (parsedSamplePayload !== undefined && !hasItemsArray(parsedSamplePayload)) {
    diagnostics.push({
      level: "error",
      message: "Sample payload must include an items array."
    });
  }

  return diagnostics;
}

function validateAdapterConfig(
  diagnostics: ConnectorConfigDiagnostic[],
  target: string,
  config: JsonRecord
) {
  if (!Object.prototype.hasOwnProperty.call(config, "adapter")) {
    return;
  }

  const adapterValue = config.adapter;
  if (typeof adapterValue !== "string") {
    diagnostics.push({
      level: "error",
      message: "adapter must be a string."
    });
    return;
  }

  const adapter = adapterValue.trim();
  if (!adapter) {
    diagnostics.push({
      level: "error",
      message: "adapter must not be empty."
    });
    return;
  }

  switch (adapter) {
    case "azure_devops":
      validateAzureDevOpsConfig(diagnostics, target, config);
      break;
    case "monitoring":
      validateMonitoringConfig(diagnostics, target, config);
      break;
    case "microsoft_graph_calendar":
    case "graph_calendar":
    case "outlook_calendar":
      validateGraphCalendarConfig(diagnostics, target, config);
      break;
    case "microsoft_graph_mail":
    case "graph_mail":
    case "outlook_mail":
      validateGraphMailConfig(diagnostics, target, config);
      break;
    case "erp_private_messages":
    case "erp_messages_http":
    case "erp_http":
      validateErpPrivateMessagesConfig(diagnostics, target, config);
      break;
    case "calendar_sample":
    case "calendar":
      validateSampleNotificationConfig(diagnostics, target, config, "events", [
        "calendar_events",
        "notifications"
      ]);
      break;
    case "outlook_mail_sample":
    case "outlook":
    case "erp_messages_sample":
    case "erp_messages":
    case "erp":
      validateSampleNotificationConfig(diagnostics, target, config, "messages", ["notifications"]);
      break;
    default:
      diagnostics.push({
        level: "error",
        message: `Adapter ${adapter} is not supported.`
      });
  }
}

function validateAzureDevOpsConfig(
  diagnostics: ConnectorConfigDiagnostic[],
  target: string,
  config: JsonRecord
) {
  requireTarget(diagnostics, "azure_devops", target, "work_cards");

  if (!hasAllNonEmpty(config, ["wiql_url", "work_items_url"])) {
    requireNonEmpty(diagnostics, config, "organization", "is required unless both endpoint URLs are set");
    requireNonEmpty(diagnostics, config, "project", "is required unless both endpoint URLs are set");
  }

  validateUrlFields(diagnostics, config, [
    "wiql_url",
    "work_items_url",
    "base_url",
    "web_url_base"
  ]);
  validatePositiveInteger(diagnostics, config, "timeout_seconds");
  validateIntegerRange(diagnostics, config, "max_items", 1, 10000);
}

function validateMonitoringConfig(
  diagnostics: ConnectorConfigDiagnostic[],
  target: string,
  config: JsonRecord
) {
  requireTarget(diagnostics, "monitoring", target, "service_health");
  requireUrlAny(diagnostics, config, ["url"]);
  validatePositiveInteger(diagnostics, config, "default_maintainer_id");
  validatePositiveInteger(diagnostics, config, "timeout_seconds");
}

function validateGraphCalendarConfig(
  diagnostics: ConnectorConfigDiagnostic[],
  target: string,
  config: JsonRecord
) {
  requireOneOfTargets(diagnostics, "microsoft_graph_calendar", target, [
    "calendar_events",
    "notifications"
  ]);
  validateUrlFields(diagnostics, config, [
    "calendar_view_url",
    "base_url",
    "token_url",
    "authorization_url"
  ]);
  validatePositiveInteger(diagnostics, config, "timeout_seconds");
  validateIntegerRange(diagnostics, config, "top", 1, 50);
  validateIntegerRange(diagnostics, config, "lookahead_hours", 1, 168);
  validateIntegerRange(diagnostics, config, "max_pages", 1, 100);
  validateIntegerRange(diagnostics, config, "max_items", 1, 10000);
}

function validateGraphMailConfig(
  diagnostics: ConnectorConfigDiagnostic[],
  target: string,
  config: JsonRecord
) {
  requireTarget(diagnostics, "microsoft_graph_mail", target, "notifications");
  validateUrlFields(diagnostics, config, [
    "messages_url",
    "mail_messages_url",
    "base_url",
    "token_url",
    "authorization_url"
  ]);
  validatePositiveInteger(diagnostics, config, "timeout_seconds");
  validateIntegerRange(diagnostics, config, "top", 1, 50);
  validateIntegerRange(diagnostics, config, "lookback_hours", 1, 720);
  validateIntegerRange(diagnostics, config, "max_pages", 1, 100);
  validateIntegerRange(diagnostics, config, "max_items", 1, 10000);
}

function validateErpPrivateMessagesConfig(
  diagnostics: ConnectorConfigDiagnostic[],
  target: string,
  config: JsonRecord
) {
  requireTarget(diagnostics, "erp_private_messages", target, "notifications");
  requireUrlAny(diagnostics, config, ["messages_url", "private_messages_url", "url"]);
  validatePositiveInteger(diagnostics, config, "timeout_seconds");
  validateIntegerRange(diagnostics, config, "top", 1, 100);
  validateIntegerRange(diagnostics, config, "limit", 1, 100);
  validateIntegerRange(diagnostics, config, "lookback_hours", 1, 720);
  validateBoolean(diagnostics, config, "snapshot_complete");

  const apiKeyHeader = stringField(config, "api_key_header");
  if (apiKeyHeader && !/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(apiKeyHeader)) {
    diagnostics.push({
      level: "error",
      message: "api_key_header must be a valid HTTP header name."
    });
  }
}

function validateSampleNotificationConfig(
  diagnostics: ConnectorConfigDiagnostic[],
  target: string,
  config: JsonRecord,
  itemField: string,
  targets: string[]
) {
  requireOneOfTargets(diagnostics, "sample notification adapter", target, targets);

  const items = config[itemField];
  if (items !== undefined && !Array.isArray(items)) {
    diagnostics.push({
      level: "error",
      message: `${itemField} must be an array when provided.`
    });
  }
}

function requireTarget(
  diagnostics: ConnectorConfigDiagnostic[],
  adapter: string,
  target: string,
  expected: string
) {
  if (target !== expected) {
    diagnostics.push({
      level: "error",
      message: `${adapter} config requires target ${expected}.`
    });
  }
}

function requireOneOfTargets(
  diagnostics: ConnectorConfigDiagnostic[],
  adapter: string,
  target: string,
  expected: string[]
) {
  if (!expected.includes(target)) {
    diagnostics.push({
      level: "error",
      message: `${adapter} config requires target ${expected.join(" or ")}.`
    });
  }
}

function requireUrlAny(
  diagnostics: ConnectorConfigDiagnostic[],
  config: JsonRecord,
  fields: string[]
) {
  if (!fields.some((field) => hasNonEmpty(config, field))) {
    diagnostics.push({
      level: "error",
      message: `Set one of ${fields.join(", ")}.`
    });
    return;
  }

  validateUrlFields(diagnostics, config, fields);
}

function requireNonEmpty(
  diagnostics: ConnectorConfigDiagnostic[],
  config: JsonRecord,
  field: string,
  reason: string
) {
  if (!hasNonEmpty(config, field)) {
    diagnostics.push({
      level: "error",
      message: `${field} ${reason}.`
    });
  }
}

function validateUrlFields(
  diagnostics: ConnectorConfigDiagnostic[],
  config: JsonRecord,
  fields: string[]
) {
  fields.forEach((field) => {
    const value = stringField(config, field);
    if (value && !isAbsoluteHttpUrl(value)) {
      diagnostics.push({
        level: "error",
        message: `${field} must be an absolute HTTP URL.`
      });
    }
  });
}

function validatePositiveInteger(
  diagnostics: ConnectorConfigDiagnostic[],
  config: JsonRecord,
  field: string
) {
  const value = config[field];

  if (value !== undefined && (!Number.isInteger(value) || Number(value) <= 0)) {
    diagnostics.push({
      level: "error",
      message: `${field} must be a positive integer.`
    });
  }
}

function validateIntegerRange(
  diagnostics: ConnectorConfigDiagnostic[],
  config: JsonRecord,
  field: string,
  min: number,
  max: number
) {
  const value = config[field];

  if (value !== undefined && (!Number.isInteger(value) || Number(value) < min || Number(value) > max)) {
    diagnostics.push({
      level: "error",
      message: `${field} must be an integer from ${min} to ${max}.`
    });
  }
}

function validateBoolean(
  diagnostics: ConnectorConfigDiagnostic[],
  config: JsonRecord,
  field: string
) {
  const value = config[field];

  if (value !== undefined && typeof value !== "boolean") {
    diagnostics.push({
      level: "error",
      message: `${field} must be a boolean.`
    });
  }
}

function parseJsonObject(
  value: string,
  label: string,
  diagnostics: ConnectorConfigDiagnostic[]
): JsonRecord | null {
  const parsed = parseJsonValue(value, label, diagnostics);

  if (parsed === undefined) {
    return null;
  }

  if (!isRecord(parsed)) {
    diagnostics.push({
      level: "error",
      message: `${label} must be a JSON object.`
    });
    return null;
  }

  return parsed;
}

function parseJsonValue(
  value: string,
  label: string,
  diagnostics: ConnectorConfigDiagnostic[]
): unknown {
  try {
    return JSON.parse(value);
  } catch {
    diagnostics.push({
      level: "error",
      message: `${label} must be valid JSON.`
    });
    return undefined;
  }
}

function hasItemsArray(value: unknown) {
  return isRecord(value) && Array.isArray(value.items);
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringField(config: JsonRecord, field: string): string {
  const value = config[field];

  return typeof value === "string" ? value.trim() : "";
}

function hasNonEmpty(config: JsonRecord, field: string) {
  return stringField(config, field).length > 0;
}

function hasAllNonEmpty(config: JsonRecord, fields: string[]) {
  return fields.every((field) => hasNonEmpty(config, field));
}

function isAbsoluteHttpUrl(value: string) {
  try {
    const url = new URL(value);

    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}
