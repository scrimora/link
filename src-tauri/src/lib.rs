mod app_state;
mod bridge;
mod deep_link;
mod lcu;
mod lockfile;
mod messages;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
use url::Url;

use crate::app_state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new();

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = show_main_window(app);
        }));
    }

    let mut updater = tauri_plugin_updater::Builder::new();
    if let Some(public_key) = bundled_updater_public_key() {
        updater = updater.pubkey(public_key);
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(updater.build())
        .setup(move |app| {
            let state = state.clone();
            app.manage(state.clone());

            bridge::spawn(state.clone())?;
            deep_link::register(&app.handle().clone(), state)?;
            setup_tray(app)?;

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = check_for_updates(handle).await;
            });

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
    let show = MenuItem::with_id(app, "show", "Open Scrimora Link", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let handle = app.handle().clone();

    TrayIconBuilder::with_id("scrimora-link")
        .menu(&menu)
        .on_menu_event(
            move |app: &tauri::AppHandle, event| match event.id.as_ref() {
                "show" => {
                    let _ = show_main_window(app);
                }
                "quit" => app.exit(0),
                _ => {}
            },
        )
        .on_tray_icon_event(move |tray: &tauri::tray::TrayIcon, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let _ = show_main_window(&handle);
                let _ = tray.set_tooltip(Some("Scrimora Link"));
            }
        })
        .build(app)?;

    Ok(())
}

fn show_main_window<R: tauri::Runtime>(app: &impl Manager<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.set_focus()?;
    }

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
