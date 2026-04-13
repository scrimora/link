use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientMessage {
    Hello { nonce: String, origin: String },
    GetRecentCustomGames,
    ImportGame { game_id: i64 },
    Ping,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerMessage {
    Ready {
        companion_version: String,
        bridge_port: u16,
    },
    RecentCustomGames {
        games: Vec<RecentGameSummary>,
    },
    ImportPayload {
        game_payload: Value,
        timeline_payload: Value,
        source_context: SourceContext,
    },
    Error {
        code: &'static str,
        message: String,
    },
    Pong,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecentGameSummary {
    pub game_id: i64,
    pub played_at: String,
    pub duration_minutes: i64,
    pub game_version: Option<String>,
    pub queue_label: Option<String>,
    pub observer_label: Option<String>,
    pub participant_names: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceContext {
    pub observer_puuid: Option<String>,
    pub observer_riot_id: Option<String>,
    pub platform_id: Option<String>,
    pub region: Option<String>,
    pub game_id: i64,
    pub client_version: Option<String>,
    pub transport: &'static str,
}
