mod app_state;
mod bridge;
mod deep_link;
mod lcu;
mod lockfile;
mod messages;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
use url::Url;

use crate::app_state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new();

    let mut builder = tauri::Builder::default();
    let updater_public_key = bundled_updater_public_key().map(str::to_string);

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|_, _, _| {}));
    }

    if let Some(public_key) = updater_public_key.as_deref() {
        builder = builder.plugin(
            tauri_plugin_updater::Builder::new()
                .pubkey(public_key)
                .build(),
        );
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .setup(move |app| {
            let state = state.clone();
            app.manage(state.clone());

            bridge::spawn(state.clone())?;
            deep_link::register(&app.handle().clone(), state)?;
            setup_tray(app)?;

            if updater_public_key.is_some() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = check_for_updates(handle).await;
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Scrimora Link");
}

async fn check_for_updates(app: AppHandle) -> tauri_plugin_updater::Result<()> {
    let Some(endpoint) = bundled_updater_endpoint() else {
        return Ok(());
    };

    if let Some(update) = app
        .updater_builder()
        .endpoints(vec![endpoint])?
        .build()?
        .check()
        .await?
    {
        update.download_and_install(|_, _| {}, || {}).await?;
        app.restart();
    }

    Ok(())
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit])?;

    TrayIconBuilder::with_id("scrimora-link")
        .menu(&menu)
        .on_menu_event(move |app: &tauri::AppHandle, event| {
            if event.id.as_ref() == "quit" {
                app.exit(0);
            }
        })
        .build(app)?;

    Ok(())
}

fn bundled_updater_public_key() -> Option<&'static str> {
    option_env!("SCRIMORA_LINK_UPDATER_PUBLIC_KEY").filter(|value| !value.trim().is_empty())
}

fn bundled_updater_endpoint() -> Option<Url> {
    let raw_endpoint = option_env!("SCRIMORA_LINK_UPDATER_ENDPOINT")
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            option_env!("SCRIMORA_LINK_GITHUB_REPOSITORY")
                .filter(|value| !value.trim().is_empty())
                .map(|repository| {
                    format!("https://github.com/{repository}/releases/latest/download/latest.json")
                })
        })?;

    Url::parse(&raw_endpoint).ok()
}
