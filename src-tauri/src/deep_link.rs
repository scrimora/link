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

fn handle_url(url: &Url, state: Arc<AppState>) -> Result<()> {
    let nonce = url
        .query_pairs()
        .find_map(|(key, value)| (key == "nonce").then(|| value.to_string()))
        .ok_or_else(|| anyhow::anyhow!("The deep link did not include a nonce."))?;
    let origin = url
        .query_pairs()
        .find_map(|(key, value)| (key == "origin").then(|| value.to_string()))
        .ok_or_else(|| anyhow::anyhow!("The deep link did not include an origin."))?;

    state.arm_session(nonce, origin)?;

    Ok(())
}
