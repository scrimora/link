use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use serde::Serialize;
use serde_json::{Map, Value, json};
use url::Url;

const DEFAULT_UMAMI_ENDPOINT: &str = "https://analytics.scrimora.app";
const TELEMETRY_TIMEOUT: Duration = Duration::from_secs(3);
const TELEMETRY_URL: &str = "/desktop";
const TELEMETRY_TITLE: &str = "Scrimora Link";

#[derive(Clone, Debug)]
pub struct Telemetry {
    client: Client,
    endpoint: Url,
    website_id: String,
}

#[derive(Debug, Serialize)]
struct UmamiEvent<'a> {
    #[serde(rename = "type")]
    event_type: &'static str,
    payload: UmamiPayload<'a>,
}

#[derive(Debug, Serialize)]
struct UmamiPayload<'a> {
    hostname: &'static str,
    title: &'static str,
    url: &'static str,
    website: &'a str,
    name: &'a str,
    data: Value,
}

impl Telemetry {
    pub fn from_env() -> Option<Arc<Self>> {
        if telemetry_disabled() {
            return None;
        }

        let website_id = configured_value(
            "SCRIMORA_LINK_UMAMI_WEBSITE_ID",
            option_env!("SCRIMORA_LINK_UMAMI_WEBSITE_ID"),
        )?;
        let endpoint = configured_value(
            "SCRIMORA_LINK_UMAMI_ENDPOINT",
            option_env!("SCRIMORA_LINK_UMAMI_ENDPOINT"),
        )
        .unwrap_or_else(|| DEFAULT_UMAMI_ENDPOINT.to_string());
        let endpoint = Url::parse(endpoint.trim()).ok()?;

        let client = Client::builder()
            .timeout(TELEMETRY_TIMEOUT)
            .user_agent(format!("Scrimora-Link/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .ok()?;

        Some(Arc::new(Self {
            client,
            endpoint,
            website_id,
        }))
    }

    pub fn track(&self, name: &'static str) {
        self.track_with_data(name, Map::new());
    }

    pub fn track_with_data(&self, name: &'static str, mut data: Map<String, Value>) {
        data.insert("version".to_string(), json!(env!("CARGO_PKG_VERSION")));
        data.insert("os".to_string(), json!(std::env::consts::OS));
        data.insert("arch".to_string(), json!(std::env::consts::ARCH));

        let client = self.client.clone();
        let Ok(endpoint) = self.endpoint.join("/api/send") else {
            return;
        };
        let website_id = self.website_id.clone();

        tauri::async_runtime::spawn(async move {
            let event = UmamiEvent {
                event_type: "event",
                payload: UmamiPayload {
                    hostname: "scrimora-link",
                    title: TELEMETRY_TITLE,
                    url: TELEMETRY_URL,
                    website: &website_id,
                    name,
                    data: Value::Object(data),
                },
            };

            let _ = client.post(endpoint).json(&event).send().await;
        });
    }
}

fn telemetry_disabled() -> bool {
    configured_value(
        "SCRIMORA_LINK_TELEMETRY_DISABLED",
        option_env!("SCRIMORA_LINK_TELEMETRY_DISABLED"),
    )
    .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
    .unwrap_or(false)
}

fn configured_value(name: &str, compile_time_value: Option<&'static str>) -> Option<String> {
    std::env::var(name)
        .ok()
        .or_else(|| compile_time_value.map(str::to_string))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::configured_value;

    #[test]
    fn ignores_empty_configured_values() {
        assert_eq!(configured_value("MISSING_TEST_VALUE", Some("   ")), None);
    }

    #[test]
    fn trims_compile_time_values() {
        assert_eq!(
            configured_value("MISSING_TEST_VALUE", Some("  value  ")),
            Some("value".to_string())
        );
    }
}
