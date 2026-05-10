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
    id: "calendar_notifications",
    label: "Calendar notifications",
    target: "notifications",
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
          severity: "info",
          is_read: false,
          url: "https://calendar.example.test/events/platform-standup"
        }
      ]
    }
  },
  {
    id: "outlook_mail_notifications",
    label: "Outlook mail notifications",
    target: "notifications",
    schedule_cron: "@every 15m",
    config: {
      adapter: "outlook_mail_sample",
      messages: [
        {
          id: "release-brief",
          subject: "Mail: Release brief ready for review",
          from: "release-bot@example.test",
          body_preview: "API deploy window moved to 15:30.",
          importance: "high",
          web_link: "https://outlook.example.test/mail/release-brief"
        }
      ]
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
    id: "erp_message_notifications",
    label: "ERP message notifications",
    target: "notifications",
    schedule_cron: "@every 15m",
    config: {
      adapter: "erp_messages_sample",
      messages: [
        {
          id: "access-approval",
          title: "ERP: Deployment access approval waiting",
          message: "Sample ERP private message. Replace this adapter with the real ERP integration when one is available.",
          requires_approval: true
        }
      ]
    },
    sample_payload: {
      items: [
        {
          external_id: "access-approval",
          title: "ERP: Deployment access approval waiting",
          body: "Sample ERP private message. Replace this adapter with the real ERP integration when one is available.",
          severity: "warning",
          is_read: false,
          url: null
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
