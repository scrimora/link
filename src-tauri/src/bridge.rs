use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use axum::Router;
use axum::extract::{
    State,
    ws::{Message, WebSocket, WebSocketUpgrade},
};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::get;
use futures_util::StreamExt;
use tokio::net::TcpListener;

use crate::app_state::AppState;
use crate::lcu::LcuClient;
use crate::messages::{ClientMessage, ServerMessage};

const PORT_RANGE: [u16; 5] = [36130, 36131, 36132, 36133, 36134];
const SCRIMORA_LINK_WS_PROTOCOL: &str = "scrimora-link.v1";

pub fn spawn(state: Arc<AppState>) -> Result<()> {
    let listener = PORT_RANGE
        .iter()
        .find_map(|port| std::net::TcpListener::bind(("127.0.0.1", *port)).ok())
        .ok_or_else(|| anyhow!("Could not bind Scrimora Link to any loopback port."))?;

    listener
        .set_nonblocking(true)
        .context("failed to mark the loopback listener as nonblocking")?;

    state.set_bridge_port(listener.local_addr()?.port());

    let app = Router::new()
        .route("/", get(websocket_handler))
        .with_state(state);

    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("failed to attach the local bridge listener to Tokio: {error}");
                return;
            }
        };

        let _ = axum::serve(listener, app).await;
    });

    Ok(())
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let origin = headers
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    ws.protocols([SCRIMORA_LINK_WS_PROTOCOL])
        .on_upgrade(move |socket| handle_socket(socket, origin, state))
}

async fn handle_socket(mut socket: WebSocket, header_origin: Option<String>, state: Arc<AppState>) {
    let Some(Ok(Message::Text(first_message))) = socket.next().await else {
        return;
    };

    let Ok(ClientMessage::Hello { nonce, origin }) =
        serde_json::from_str::<ClientMessage>(&first_message)
    else {
        let _ = send_message(
            &mut socket,
            ServerMessage::Error {
                code: "invalid_handshake",
                message: "The first local bridge message must be HELLO.".to_string(),
            },
        )
        .await;
        return;
    };

    if let Err(error) = state.verify_session(&nonce, &origin, header_origin.as_deref()) {
        let _ = send_message(
            &mut socket,
            ServerMessage::Error {
                code: "unauthorized_origin",
                message: error.to_string(),
            },
        )
        .await;
        return;
    }

    let _ = send_message(
        &mut socket,
        ServerMessage::Ready {
            companion_version: env!("CARGO_PKG_VERSION").to_string(),
            bridge_port: state.bridge_port().unwrap_or_default(),
        },
    )
    .await;

    while let Some(Ok(message)) = socket.next().await {
        let Message::Text(text) = message else {
            continue;
        };

        let parsed = serde_json::from_str::<ClientMessage>(&text);

        match parsed {
            Ok(ClientMessage::GetRecentCustomGames) => match LcuClient::discover() {
                Ok(client) => match client.recent_custom_games().await {
                    Ok(games) => {
                        let _ =
                            send_message(&mut socket, ServerMessage::RecentCustomGames { games })
                                .await;
                    }
                    Err(error) => {
                        let _ = send_message(
                            &mut socket,
                            ServerMessage::Error {
                                code: "lcu_recent_games_failed",
                                message: error.to_string(),
                            },
                        )
                        .await;
                    }
                },
                Err(error) => {
                    let _ = send_message(
                        &mut socket,
                        ServerMessage::Error {
                            code: "lcu_recent_games_failed",
                            message: error.to_string(),
                        },
                    )
                    .await;
                }
            },
            Ok(ClientMessage::ImportGame { game_id }) => match LcuClient::discover() {
                Ok(client) => match client.import_game(game_id).await {
                    Ok(bundle) => {
                        let _ = send_message(
                            &mut socket,
                            ServerMessage::ImportPayload {
                                game_payload: bundle.game_payload,
                                timeline_payload: bundle.timeline_payload,
                                source_context: bundle.source_context,
                            },
                        )
                        .await;
                    }
                    Err(error) => {
                        let _ = send_message(
                            &mut socket,
                            ServerMessage::Error {
                                code: "lcu_import_failed",
                                message: error.to_string(),
                            },
                        )
                        .await;
                    }
                },
                Err(error) => {
                    let _ = send_message(
                        &mut socket,
                        ServerMessage::Error {
                            code: "lcu_import_failed",
                            message: error.to_string(),
                        },
                    )
                    .await;
                }
            },
            Ok(ClientMessage::Ping) => {
                let _ = send_message(&mut socket, ServerMessage::Pong).await;
            }
            Ok(ClientMessage::Hello { .. }) | Err(_) => {
                let _ = send_message(
                    &mut socket,
                    ServerMessage::Error {
                        code: "invalid_message",
                        message: "The local bridge message could not be parsed.".to_string(),
                    },
                )
                .await;
            }
        }
    }
}

async fn send_message(socket: &mut WebSocket, message: ServerMessage) -> Result<()> {
    socket
        .send(Message::Text(serde_json::to_string(&message)?.into()))
        .await
        .map_err(|error| anyhow!(error.to_string()))
}
