use std::sync::Arc;

use anyhow::Result;
use tauri::AppHandle;
use tauri_plugin_deep_link::DeepLinkExt;
use url::Url;

use crate::app_state::AppState;

pub fn register(app: &AppHandle, state: Arc<AppState>) -> tauri::Result<()> {
    #[cfg(windows)]
    app.deep_link()
        .register_all()
        .map_err(anyhow::Error::from)?;

    if let Some(urls) = app.deep_link().get_current().map_err(anyhow::Error::from)? {
        for url in urls {
            let _ = handle_url(&url, state.clone());
        }
    }

    let state_for_events = state.clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            let _ = handle_url(&url, state_for_events.clone());
        }
    });

    Ok(())
}

pub fn handle_url(url: &Url, state: Arc<AppState>) -> Result<()> {
    let (nonce, origin) = parse_import_session(url)?;

    state.arm_session(nonce, origin)?;

    Ok(())
}

pub fn handle_url_str(raw_url: &str, state: Arc<AppState>) -> Result<()> {
    let url = Url::parse(raw_url)?;

    handle_url(&url, state)
}

fn parse_import_session(url: &Url) -> Result<(String, String)> {
    let nonce = url
        .query_pairs()
        .find_map(|(key, value)| (key == "nonce").then(|| value.to_string()))
        .ok_or_else(|| anyhow::anyhow!("The deep link did not include a nonce."))?;
    let origin = url
        .query_pairs()
        .find_map(|(key, value)| (key == "origin").then(|| value.to_string()))
        .ok_or_else(|| anyhow::anyhow!("The deep link did not include an origin."))?;

    Ok((nonce, origin))
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::parse_import_session;

    #[test]
    fn parses_nonce_and_origin_from_import_links() {
        let url = Url::parse(
            "scrimora-link://import?nonce=session-123&origin=https%3A%2F%2Fdev.scrimora.app",
        )
        .expect("deep link to parse");

        let (nonce, origin) = parse_import_session(&url).expect("session parameters to parse");

        assert_eq!(nonce, "session-123");
        assert_eq!(origin, "https://dev.scrimora.app");
    }
}
