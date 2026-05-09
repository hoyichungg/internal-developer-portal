import { prettyJson } from "../../utils/format";

export const defaultConnectorConfig = {
  target: "work_cards",
  enabled: true,
  schedule_cron: "",
  config: JSON.stringify({ adapter: "azure_devops" }, null, 2),
  sample_payload: JSON.stringify({ items: [] }, null, 2)
};

export const connectorTemplates = [
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
    id: "notification_feed",
    label: "Notification feed",
    target: "notifications",
    schedule_cron: "",
    config: {},
    sample_payload: {
      items: [
        {
          external_id: "erp-approval-1",
          title: "ERP approval waiting for platform team",
          body: "A deployment access request needs review.",
          severity: "warning",
          is_read: false,
          url: "https://erp.example.test/messages/erp-approval-1"
        }
      ]
    }
  }
];

export function connectorConfigFromResponse(response) {
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

export function connectorConfigFromTemplate(templateId) {
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
