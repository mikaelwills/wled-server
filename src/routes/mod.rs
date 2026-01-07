mod audio;
mod boards;
mod effects;
pub mod groups;
mod patterns;
mod presets;
mod programs;
mod settings;

use axum::{
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, post, put},
    Json, Router,
};
use futures::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tracing::info;

use crate::board::BoardState;
use crate::config::Config;
use crate::types::SharedState;

pub use settings::send_osc_sync;

pub fn build_api_router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(hello))
        .route("/boards", get(list_boards).post(boards::register_board))
        .route("/boards/:id", put(boards::update_board).delete(boards::delete_board))
        .route("/groups", post(groups::create_group))
        .route("/groups/:id", put(groups::update_group).delete(groups::delete_group))
        .route("/group/:id/power", post(groups::set_group_power))
        .route("/group/:id/brightness", post(groups::set_group_brightness))
        .route("/group/:id/color", post(groups::set_group_color))
        .route("/group/:id/effect", post(groups::set_group_effect))
        .route("/group/:id/preset", post(groups::set_group_preset))
        .route("/group/:id/presets/sync", post(groups::sync_presets_to_group))
        .route("/board/:id/power", post(boards::set_board_power))
        .route("/board/:id/brightness", post(boards::set_brightness))
        .route("/board/:id/color", post(boards::set_color))
        .route("/board/:id/effect", post(boards::set_effect))
        .route("/board/:id/speed", post(boards::set_speed))
        .route("/board/:id/intensity", post(boards::set_intensity))
        .route("/board/:id/preset", post(boards::set_preset))
        .route("/board/:id/presets", get(boards::get_board_presets))
        .route("/board/:id/presets/:slot", delete(boards::delete_board_preset))
        .route("/board/:id/led-count", post(boards::set_led_count))
        .route("/board/:id/transition", post(boards::set_transition))
        .route("/board/:id/reset-segment", post(boards::reset_segment))
        .route("/board/:id/presets/sync", post(boards::sync_presets_to_board))
        .route("/board/:id/presets/replace", post(boards::replace_presets_on_board))
        .route("/events", get(sse_handler))
        .route("/programs", post(programs::save_program))
        .route("/programs", get(programs::list_programs))
        .route("/programs/:id", get(programs::get_program))
        .route("/programs/:id", delete(programs::delete_program))
        .route("/programs/:id", put(programs::update_program))
        .route("/programs/:id/play", post(programs::play_program))
        .route("/programs/stop", post(programs::stop_program))
        .route("/presets", post(presets::save_preset).get(presets::list_presets))
        .route("/presets/:id", get(presets::get_preset).put(presets::update_preset).delete(presets::delete_preset))
        .route("/audio/:id", post(audio::upload_audio).get(audio::get_audio).delete(audio::delete_audio))
        .route("/osc", post(settings::send_osc))
        .route("/settings/loopy-pro", get(settings::get_loopy_pro_settings).put(settings::update_loopy_pro_settings))
        .route("/effects/start", post(effects::start_effects_engine))
        .route("/effects/stop", post(effects::stop_effects_engine))
        .route("/effects/presets", get(effects::list_effect_presets))
        .route("/patterns/presets", get(patterns::list_pattern_presets))
        .route("/patterns/start", post(patterns::start_pattern))
        .route("/patterns/stop", post(patterns::stop_pattern))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .with_state(state)
}

async fn hello() -> &'static str {
    info!("Health check called");
    "WLED Server Running"
}

async fn sse_handler(
    State(state): State<SharedState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.broadcast_tx.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(event) => {
                match serde_json::to_string(&event) {
                    Ok(data) => Some(Ok(Event::default().data(data))),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(serde::Serialize)]
struct BoardResponse {
    id: String,
    ip: String,
    #[serde(flatten)]
    state: BoardState,
}

#[derive(serde::Serialize)]
struct GroupResponse {
    id: String,
    members: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    universe: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    power: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    brightness: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<[u8; 3]>,
}

#[derive(serde::Serialize)]
pub(crate) struct ListBoardsResponse {
    boards: Vec<BoardResponse>,
    groups: Vec<GroupResponse>,
}

pub async fn list_boards(
    State(state): State<SharedState>,
) -> Result<Json<ListBoardsResponse>, StatusCode> {
    let senders = state.boards.read().await;
    let mut boards = Vec::new();

    for (id, entry) in senders.iter() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if entry
            .sender
            .send(crate::board::BoardCommand::GetState(tx))
            .await
            .is_ok()
        {
            match tokio::time::timeout(std::time::Duration::from_millis(1000), rx).await {
                Ok(Ok(board_state)) => {
                    boards.push(BoardResponse {
                        id: id.clone(),
                        ip: entry.ip.clone(),
                        state: board_state,
                    });
                }
                _ => {
                    boards.push(BoardResponse {
                        id: id.clone(),
                        ip: entry.ip.clone(),
                        state: BoardState::new(id.clone(), entry.ip.clone()),
                    });
                }
            }
        }
    }

    let config = Config::load().unwrap_or(Config {
        boards: vec![],
        groups: vec![],
        loopy_pro: crate::config::LoopyProConfig::default(),
        effect_presets: vec![],
        pattern_presets: vec![],
    });

    let groups: Vec<GroupResponse> = config
        .groups
        .iter()
        .map(|group| {
            let member_states: Vec<&BoardResponse> = group
                .members
                .iter()
                .filter_map(|member_id| boards.iter().find(|b| &b.id == member_id))
                .collect();

            let all_on = !member_states.is_empty()
                && member_states.iter().all(|s| s.state.on && s.state.connected);
            let avg_brightness = if member_states.is_empty() {
                None
            } else {
                Some(
                    (member_states.iter().map(|s| s.state.brightness as u32).sum::<u32>()
                        / member_states.len() as u32) as u8,
                )
            };
            let first_color = member_states.first().map(|s| s.state.color);

            GroupResponse {
                id: group.id.clone(),
                members: group.members.clone(),
                universe: group.universe,
                power: Some(all_on),
                brightness: avg_brightness,
                color: first_color,
            }
        })
        .collect();

    Ok(Json(ListBoardsResponse { boards, groups }))
}
