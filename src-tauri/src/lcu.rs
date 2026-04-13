use std::collections::HashSet;
use std::time::Duration;

use anyhow::{Result, anyhow};
use base64::Engine;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;

use crate::app_state::LcuConnectionStatus;
use crate::lockfile::{LockfileCredentials, discover_lockfile};
use crate::messages::{RecentGameSummary, SourceContext};

const LCU_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const LCU_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const RECENT_CUSTOM_GAME_LIMIT: usize = 20;

pub struct LcuClient {
    credentials: LockfileCredentials,
    client: Client,
}

pub struct ImportBundle {
    pub game_payload: Value,
    pub timeline_payload: Value,
    pub source_context: SourceContext,
}

impl LcuClient {
    pub fn discover() -> Result<Self> {
        let credentials = discover_lockfile()?;
        let client = Client::builder()
            .connect_timeout(LCU_CONNECT_TIMEOUT)
            .danger_accept_invalid_certs(true)
            .timeout(LCU_REQUEST_TIMEOUT)
            .user_agent(format!("Scrimora-Link/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            credentials,
            client,
        })
    }

    pub async fn recent_custom_games(&self) -> Result<Vec<RecentGameSummary>> {
        let summoner = self.current_summoner().await?;
        let puuid = summoner
            .get("puuid")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("The current League account did not expose a PUUID."))?;
        let observer_label = riot_id_label(&summoner);

        let history = self
            .get_json(&format!(
                "/lol-match-history/v1/products/lol/{puuid}/matches"
            ))
            .await?;

        Ok(extract_recent_custom_games(&history, observer_label))
    }

    pub async fn import_game(&self, game_id: i64) -> Result<ImportBundle> {
        let summoner = self.current_summoner().await?;
        let observer_label = riot_id_label(&summoner);
        let observer_puuid = summoner
            .get("puuid")
            .and_then(Value::as_str)
            .map(str::to_string);

        let game_payload = self
            .get_json(&format!("/lol-match-history/v1/games/{game_id}"))
            .await?;
        let timeline_payload = self
            .get_json(&format!("/lol-match-history/v1/game-timelines/{game_id}"))
            .await?;

        Ok(ImportBundle {
            source_context: SourceContext {
                observer_puuid,
                observer_riot_id: observer_label,
                platform_id: game_payload
                    .get("platformId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                region: game_payload
                    .get("platformId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                game_id,
                client_version: game_payload
                    .get("gameVersion")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                transport: "websocket_loopback",
            },
            game_payload,
            timeline_payload,
        })
    }

    async fn current_summoner(&self) -> Result<Value> {
        self.get_json("/lol-summoner/v1/current-summoner").await
    }

    async fn get_json(&self, path: &str) -> Result<Value> {
        let response = self
            .client
            .get(format!(
                "{}://127.0.0.1:{}{}",
                self.credentials.protocol, self.credentials.port, path
            ))
            .header(
                "Authorization",
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD
                        .encode(format!("riot:{}", self.credentials.password))
                ),
            )
            .send()
            .await?;

        let response = response.error_for_status()?;

        Ok(response.json().await?)
    }
}

pub async fn detect_connection_status() -> LcuConnectionStatus {
    let Ok(client) = LcuClient::discover() else {
        return LcuConnectionStatus::Disconnected;
    };

    match client.current_summoner().await {
        Ok(_) => LcuConnectionStatus::Connected,
        Err(_) => LcuConnectionStatus::Connecting,
    }
}

fn riot_id_label(summoner: &Value) -> Option<String> {
    let game_name = summoner
        .get("gameName")
        .and_then(Value::as_str)
        .or_else(|| summoner.get("displayName").and_then(Value::as_str))?;
    let tag_line = summoner
        .get("tagLine")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if tag_line.is_empty() {
        Some(game_name.to_string())
    } else {
        Some(format!("{game_name}#{tag_line}"))
    }
}

fn extract_recent_custom_games(
    history: &Value,
    observer_label: Option<String>,
) -> Vec<RecentGameSummary> {
    let mut seen_game_ids = HashSet::new();
    let mut summaries = history
        .pointer("/games/games")
        .and_then(Value::as_array)
        .or_else(|| history.get("games").and_then(Value::as_array))
        .into_iter()
        .flatten()
        .filter(|game| is_custom_game(game))
        .filter_map(|game| summarize_game(game, observer_label.clone()))
        .filter(|summary| seen_game_ids.insert(summary.game_id))
        .collect::<Vec<_>>();

    summaries.sort_by(|left, right| {
        right
            .played_at
            .cmp(&left.played_at)
            .then_with(|| right.game_id.cmp(&left.game_id))
    });
    summaries.truncate(RECENT_CUSTOM_GAME_LIMIT);

    summaries
}

fn is_custom_game(game: &Value) -> bool {
    let queue_id = game.get("queueId").and_then(Value::as_i64);
    let game_type = game
        .get("gameType")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_uppercase();

    queue_id == Some(0)
        || matches!(
            game_type.as_str(),
            "CUSTOM_GAME" | "PRACTICE_GAME" | "PRIVATE_GAME"
        )
}

fn summarize_game(game: &Value, observer_label: Option<String>) -> Option<RecentGameSummary> {
    let game_id = game.get("gameId").and_then(Value::as_i64)?;
    let played_at = ms_to_rfc3339(
        game.get("gameCreation")
            .and_then(Value::as_i64)
            .or_else(|| game.get("gameCreationDate").and_then(Value::as_i64))
            .unwrap_or_default(),
    )?;
    let duration_minutes = game
        .get("gameDuration")
        .and_then(Value::as_i64)
        .or_else(|| game.get("gameLength").and_then(Value::as_i64))
        .map(|value| {
            if value > 100_000 {
                value / 60_000
            } else {
                value / 60
            }
        })
        .unwrap_or_default();
    let participant_names = game
        .get("participants")
        .and_then(Value::as_array)
        .map(|participants| {
            participants
                .iter()
                .filter_map(|participant| {
                    participant
                        .get("riotIdGameName")
                        .and_then(Value::as_str)
                        .or_else(|| participant.get("summonerName").and_then(Value::as_str))
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(RecentGameSummary {
        game_id,
        played_at,
        duration_minutes,
        game_version: game
            .get("gameVersion")
            .and_then(Value::as_str)
            .map(str::to_string),
        queue_label: game
            .get("gameType")
            .and_then(Value::as_str)
            .map(str::to_string),
        observer_label,
        participant_names,
    })
}

fn ms_to_rfc3339(value: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(value).map(|timestamp| timestamp.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::extract_recent_custom_games;

    #[test]
    fn filters_recent_custom_games_from_match_history() {
        let history = json!({
            "games": {
                "games": [
                    {
                        "gameId": 101,
                        "gameCreation": 1_710_000_000_000_i64,
                        "gameDuration": 1800,
                        "gameType": "CUSTOM_GAME",
                        "participants": [
                            { "riotIdGameName": "TopMain" }
                        ]
                    },
                    {
                        "gameId": 202,
                        "gameCreation": 1_710_000_000_000_i64,
                        "gameDuration": 1800,
                        "gameType": "MATCHED_GAME",
                        "participants": []
                    }
                ]
            }
        });

        let summaries = extract_recent_custom_games(&history, Some("Coach#EUW".to_string()));

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].game_id, 101);
        assert_eq!(summaries[0].observer_label.as_deref(), Some("Coach#EUW"));
    }

    #[test]
    fn sorts_and_deduplicates_recent_custom_games() {
        let history = json!({
            "games": {
                "games": [
                    {
                        "gameId": 101,
                        "queueId": 0,
                        "gameCreation": 1_710_000_000_000_i64,
                        "gameDuration": 1800,
                        "gameType": "CUSTOM_GAME",
                        "participants": [{ "riotIdGameName": "TopMain" }]
                    },
                    {
                        "gameId": 202,
                        "queueId": 0,
                        "gameCreation": 1_720_000_000_000_i64,
                        "gameDuration": 1800,
                        "gameType": "CUSTOM_GAME",
                        "participants": [{ "riotIdGameName": "MidMain" }]
                    },
                    {
                        "gameId": 202,
                        "queueId": 0,
                        "gameCreation": 1_720_000_000_000_i64,
                        "gameDuration": 1800,
                        "gameType": "CUSTOM_GAME",
                        "participants": [{ "riotIdGameName": "MidMain" }]
                    }
                ]
            }
        });

        let summaries = extract_recent_custom_games(&history, None);

        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].game_id, 202);
        assert_eq!(summaries[1].game_id, 101);
    }
}
