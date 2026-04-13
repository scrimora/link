use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LcuConnectionStatus {
    Connecting,
    Connected,
    Disconnected,
}

impl LcuConnectionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Disconnected => "Disconnected",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PendingSession {
    pub nonce: String,
    pub origin: String,
    pub expires_at: Instant,
}

#[derive(Debug)]
pub struct AppState {
    allowed_origins: Vec<String>,
    pending_session: RwLock<Option<PendingSession>>,
    bridge_port: RwLock<Option<u16>>,
    lcu_status: RwLock<LcuConnectionStatus>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            allowed_origins: default_allowed_origins(),
            pending_session: RwLock::new(None),
            bridge_port: RwLock::new(None),
            lcu_status: RwLock::new(LcuConnectionStatus::Connecting),
        })
    }

    pub fn arm_session(&self, nonce: String, origin: String) -> Result<()> {
        let normalized_origin = normalize_origin(&origin)
            .ok_or_else(|| anyhow!("Origin [{origin}] is not a valid HTTP(S) origin."))?;

        if !self.is_allowed_origin(&normalized_origin) {
            return Err(anyhow!("Origin [{origin}] is not allowed."));
        }

        *self
            .pending_session
            .write()
            .expect("pending session lock poisoned") = Some(PendingSession {
            nonce,
            origin: normalized_origin,
            expires_at: Instant::now() + Duration::from_secs(300),
        });

        Ok(())
    }

    pub fn verify_session(
        &self,
        nonce: &str,
        claimed_origin: &str,
        header_origin: Option<&str>,
    ) -> Result<()> {
        let mut pending_session = self
            .pending_session
            .write()
            .expect("pending session lock poisoned");
        let session = pending_session
            .as_ref()
            .ok_or_else(|| anyhow!("No active import session is registered."))?;
        let normalized_claimed_origin = normalize_origin(claimed_origin)
            .ok_or_else(|| anyhow!("The claimed origin is not a valid HTTP(S) origin."))?;
        let normalized_header_origin = header_origin
            .and_then(normalize_origin)
            .ok_or_else(|| anyhow!("The websocket Origin header was missing or invalid."))?;

        if Instant::now() > session.expires_at {
            return Err(anyhow!(
                "The import session expired. Re-open Scrimora Link."
            ));
        }

        if session.nonce != nonce {
            return Err(anyhow!(
                "The import nonce does not match the active session."
            ));
        }

        if session.origin != normalized_claimed_origin {
            return Err(anyhow!(
                "The claimed origin does not match the deep-link origin."
            ));
        }

        if normalized_header_origin != session.origin {
            return Err(anyhow!(
                "The websocket Origin header did not match the paired site."
            ));
        }

        if !self.is_allowed_origin(&session.origin) {
            return Err(anyhow!("The requesting origin is not in the allowlist."));
        }

        pending_session.take();

        Ok(())
    }

    pub fn set_bridge_port(&self, port: u16) {
        *self.bridge_port.write().expect("bridge port lock poisoned") = Some(port);
    }

    pub fn bridge_port(&self) -> Option<u16> {
        *self.bridge_port.read().expect("bridge port lock poisoned")
    }

    pub fn set_lcu_status(&self, status: LcuConnectionStatus) {
        *self.lcu_status.write().expect("lcu status lock poisoned") = status;
    }

    pub fn lcu_status(&self) -> LcuConnectionStatus {
        *self.lcu_status.read().expect("lcu status lock poisoned")
    }

    fn is_allowed_origin(&self, origin: &str) -> bool {
        self.allowed_origins.iter().any(|allowed| allowed == origin)
    }
}

fn default_allowed_origins() -> Vec<String> {
    let mut origins = vec![
        "https://scrimora.app".to_string(),
        "https://dev.scrimora.app".to_string(),
        "http://localhost:3000".to_string(),
        "http://127.0.0.1:3000".to_string(),
        "http://localhost:5173".to_string(),
        "http://127.0.0.1:5173".to_string(),
        "https://localhost".to_string(),
        "https://127.0.0.1".to_string(),
    ];

    if let Ok(extra) = std::env::var("SCRIMORA_LINK_ALLOWED_ORIGINS") {
        origins.extend(
            extra
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        );
    }

    origins = origins
        .into_iter()
        .filter_map(|origin| normalize_origin(&origin))
        .collect();

    origins.sort();
    origins.dedup();

    origins
}

fn normalize_origin(origin: &str) -> Option<String> {
    let url = Url::parse(origin).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }

    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }

    let host = url.host_str()?;
    let mut normalized = format!("{}://{}", url.scheme(), host);

    if let Some(port) = url.port() {
        normalized.push(':');
        normalized.push_str(&port.to_string());
    }

    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::{AppState, LcuConnectionStatus, normalize_origin};

    #[test]
    fn verifies_the_active_paired_session() {
        let state = AppState::new();
        state
            .arm_session("nonce-1".to_string(), "https://scrimora.app".to_string())
            .expect("session to arm");

        state
            .verify_session(
                "nonce-1",
                "https://scrimora.app",
                Some("https://scrimora.app"),
            )
            .expect("session to verify");
    }

    #[test]
    fn consumes_a_session_after_a_successful_verification() {
        let state = AppState::new();
        state
            .arm_session("nonce-1".to_string(), "https://scrimora.app".to_string())
            .expect("session to arm");

        state
            .verify_session(
                "nonce-1",
                "https://scrimora.app",
                Some("https://scrimora.app"),
            )
            .expect("session to verify");

        let second_attempt = state.verify_session(
            "nonce-1",
            "https://scrimora.app",
            Some("https://scrimora.app"),
        );

        assert!(second_attempt.is_err());
    }

    #[test]
    fn normalizes_allowed_origins() {
        let normalized =
            normalize_origin("https://dev.scrimora.app/").expect("origin to normalize");

        assert_eq!(normalized, "https://dev.scrimora.app");
    }

    #[test]
    fn rejects_non_http_origins() {
        let normalized = normalize_origin("scrimora-link://import");

        assert!(normalized.is_none());
    }

    #[test]
    fn stores_the_current_lcu_status() {
        let state = AppState::new();

        state.set_lcu_status(LcuConnectionStatus::Connected);

        assert_eq!(state.lcu_status(), LcuConnectionStatus::Connected);
    }
}
