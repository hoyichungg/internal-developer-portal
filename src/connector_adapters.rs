use serde::Deserialize;
use serde_json::Value;

mod azure_devops;
mod erp;
mod graph;
mod monitoring;
mod samples;
mod shared;

use azure_devops::fetch_azure_devops_work_cards;
use erp::fetch_erp_private_messages;
use graph::{fetch_microsoft_graph_calendar_events, fetch_microsoft_graph_mail_messages};
use monitoring::fetch_monitoring_service_health;
use samples::{fetch_sample_notifications, SampleNotificationKind};

#[derive(Deserialize)]
struct AdapterConfig {
    adapter: Option<String>,
}

pub struct ConnectorAdapterResult {
    pub payload: Option<Value>,
    pub updated_config: Option<String>,
}

pub async fn fetch_connector_payload(
    target: &str,
    config_json: &str,
) -> Result<ConnectorAdapterResult, String> {
    let adapter = serde_json::from_str::<AdapterConfig>(config_json)
        .map_err(|error| format!("connector config is not valid JSON: {error}"))?
        .adapter;

    match adapter.as_deref() {
        Some("azure_devops") if target == "work_cards" => {
            fetch_azure_devops_work_cards(config_json)
                .await
                .map(adapter_payload)
        }
        Some("azure_devops") => Err(format!(
            "azure_devops adapter does not support target {target}"
        )),
        Some("monitoring") if target == "service_health" => {
            fetch_monitoring_service_health(config_json)
                .await
                .map(adapter_payload)
        }
        Some("monitoring") => Err(format!(
            "monitoring adapter does not support target {target}"
        )),
        Some("microsoft_graph_calendar" | "graph_calendar" | "outlook_calendar") => {
            if target == "notifications" {
                fetch_microsoft_graph_calendar_events(config_json).await
            } else {
                Err(format!(
                    "microsoft_graph_calendar adapter does not support target {target}"
                ))
            }
        }
        Some("microsoft_graph_mail" | "graph_mail" | "outlook_mail") => {
            if target == "notifications" {
                fetch_microsoft_graph_mail_messages(config_json).await
            } else {
                Err(format!(
                    "microsoft_graph_mail adapter does not support target {target}"
                ))
            }
        }
        Some("erp_private_messages" | "erp_messages_http" | "erp_http")
            if target == "notifications" =>
        {
            fetch_erp_private_messages(config_json)
                .await
                .map(adapter_payload)
        }
        Some("erp_private_messages" | "erp_messages_http" | "erp_http") => Err(format!(
            "erp_private_messages adapter does not support target {target}"
        )),
        Some("calendar_sample" | "calendar") if target == "notifications" => {
            fetch_sample_notifications(config_json, SampleNotificationKind::Calendar)
                .map(adapter_payload)
        }
        Some("calendar_sample" | "calendar") => Err(format!(
            "calendar_sample adapter does not support target {target}"
        )),
        Some("outlook_mail_sample" | "outlook") if target == "notifications" => {
            fetch_sample_notifications(config_json, SampleNotificationKind::OutlookMail)
                .map(adapter_payload)
        }
        Some("outlook_mail_sample" | "outlook") => Err(format!(
            "outlook_mail_sample adapter does not support target {target}"
        )),
        Some("erp_messages_sample" | "erp_messages" | "erp") if target == "notifications" => {
            fetch_sample_notifications(config_json, SampleNotificationKind::ErpMessages)
                .map(adapter_payload)
        }
        Some("erp_messages_sample" | "erp_messages" | "erp") => Err(format!(
            "erp_messages_sample adapter does not support target {target}"
        )),
        Some(adapter) => Err(format!("connector adapter {adapter} is not supported")),
        None => Ok(ConnectorAdapterResult {
            payload: None,
            updated_config: None,
        }),
    }
}

fn adapter_payload(payload: Value) -> ConnectorAdapterResult {
    ConnectorAdapterResult {
        payload: Some(payload),
        updated_config: None,
    }
}
