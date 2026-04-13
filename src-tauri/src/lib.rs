mod app_state;
mod bridge;
mod deep_link;
mod lcu;
mod lockfile;
mod messages;

use std::process::Command;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Runtime, Wry};
use tauri_plugin_updater::UpdaterExt;
use tokio::time::sleep;
use url::Url;

use crate::app_state::{AppState, LcuConnectionStatus};

const SCRIMORA_WEBSITE_URL: &str = "https://scrimora.app";
const LCU_STATUS_POLL_INTERVAL: Duration = Duration::from_secs(5);

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
            deep_link::register(&app.handle().clone(), state.clone())?;
            let status_item = setup_tray(app)?;
            update_tray_status_item(&status_item, state.lcu_status())?;
            spawn_lcu_status_monitor(state.clone(), status_item.clone());

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

fn setup_tray(app: &mut tauri::App<Wry>) -> tauri::Result<MenuItem<Wry>> {
    let status = MenuItem::with_id(app, "status", "Status: Connecting", false, None::<&str>)?;
    let open_website_item =
        MenuItem::with_id(app, "open_website", "Open Website", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status, &open_website_item, &quit])?;

    TrayIconBuilder::with_id("scrimora-link")
        .menu(&menu)
        .on_menu_event(
            move |app: &tauri::AppHandle, event| match event.id.as_ref() {
                "open_website" => {
                    let _ = open_website();
                }
                "quit" => app.exit(0),
                _ => {}
            },
        )
        .build(app)?;

    Ok(status)
}

fn spawn_lcu_status_monitor<R: Runtime>(state: std::sync::Arc<AppState>, status_item: MenuItem<R>) {
    tauri::async_runtime::spawn(async move {
        loop {
            let status = lcu::detect_connection_status().await;
            state.set_lcu_status(status);
            let _ = update_tray_status_item(&status_item, status);
            sleep(LCU_STATUS_POLL_INTERVAL).await;
        }
    });
}

fn update_tray_status_item<R: Runtime>(
    status_item: &MenuItem<R>,
    status: LcuConnectionStatus,
) -> tauri::Result<()> {
    status_item.set_text(format!("Status: {}", status.label()))
}

fn open_website() -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        return Command::new("cmd")
            .args(["/C", "start", "", SCRIMORA_WEBSITE_URL])
            .spawn()
            .map(|_| ());
    }

    #[cfg(target_os = "macos")]
    {
        return Command::new("open")
            .arg(SCRIMORA_WEBSITE_URL)
            .spawn()
            .map(|_| ());
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(SCRIMORA_WEBSITE_URL)
            .spawn()
            .map(|_| ())
    }
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
