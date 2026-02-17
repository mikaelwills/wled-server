use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use tracing::{info, error};

use crate::audio;
use crate::setlist::{Setlist, SetlistStore};
use crate::types::SharedState;

#[derive(Serialize)]
pub struct ListSetlistsResponse {
    pub setlists: Vec<Setlist>,
    pub active_setlist_id: String,
}

pub async fn list_setlists(
    State(state): State<SharedState>,
) -> Json<ListSetlistsResponse> {
    let setlists = state.setlists.read().await;
    let active_id = state.active_setlist_id.read().await;
    let mut list = setlists.clone();
    list.sort_by_key(|s| s.display_order);
    Json(ListSetlistsResponse {
        setlists: list,
        active_setlist_id: active_id.clone(),
    })
}

#[derive(Deserialize)]
pub struct CreateSetlistRequest {
    pub name: String,
}

pub async fn create_setlist(
    State(state): State<SharedState>,
    Json(payload): Json<CreateSetlistRequest>,
) -> Result<(StatusCode, Json<Setlist>), (StatusCode, String)> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let slug: String = payload
        .name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let collapsed = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let id = format!("{}-{}", collapsed, timestamp);

    let max_order = {
        let setlists = state.setlists.read().await;
        setlists.iter().map(|s| s.display_order).max().unwrap_or(0)
    };

    let setlist = Setlist {
        id: id.clone(),
        name: payload.name,
        display_order: max_order + 1,
    };

    {
        let mut setlists = state.setlists.write().await;
        setlists.push(setlist.clone());
        save_setlists_to_disk(&state, &setlists).await?;
    }

    info!("Created setlist: {} ({})", setlist.name, setlist.id);
    Ok((StatusCode::CREATED, Json(setlist)))
}

#[derive(Deserialize)]
pub struct UpdateSetlistRequest {
    pub name: String,
}

pub async fn update_setlist(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateSetlistRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut setlists = state.setlists.write().await;
    let setlist = setlists
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Setlist {} not found", id)))?;

    setlist.name = payload.name;
    save_setlists_to_disk(&state, &setlists).await?;
    info!("Renamed setlist: {}", id);
    Ok(StatusCode::OK)
}

pub async fn delete_setlist(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    if id == "default" {
        return Err((StatusCode::BAD_REQUEST, "Cannot delete the default setlist".to_string()));
    }

    {
        let mut programs = state.programs.write().await;
        for program in programs.values_mut() {
            if program.setlist_id == id {
                program.setlist_id = "default".to_string();
                if let Err(e) = program.save_to_file(&state.storage_paths.programs) {
                    error!("Failed to save program {} after setlist delete: {}", program.id, e);
                }
            }
        }
    }

    {
        let mut setlists = state.setlists.write().await;
        setlists.retain(|s| s.id != id);
        save_setlists_to_disk(&state, &setlists).await?;
    }

    let active_id = state.active_setlist_id.read().await.clone();
    if active_id == id {
        *state.active_setlist_id.write().await = "default".to_string();
        reload_audio_for_active_setlist(&state).await;
    }

    info!("Deleted setlist: {}", id);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ReorderSetlistsRequest {
    pub setlist_ids: Vec<String>,
}

pub async fn reorder_setlists(
    State(state): State<SharedState>,
    Json(payload): Json<ReorderSetlistsRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut setlists = state.setlists.write().await;
    for (i, id) in payload.setlist_ids.iter().enumerate() {
        if let Some(setlist) = setlists.iter_mut().find(|s| &s.id == id) {
            setlist.display_order = i as i32;
        }
    }
    setlists.sort_by_key(|s| s.display_order);
    save_setlists_to_disk(&state, &setlists).await?;
    Ok(StatusCode::OK)
}

#[derive(Serialize)]
pub struct ActivateSetlistResponse {
    pub active_setlist_id: String,
    pub programs: Vec<crate::program::Program>,
}

pub async fn activate_setlist(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<ActivateSetlistResponse>, (StatusCode, String)> {
    {
        let setlists = state.setlists.read().await;
        if !setlists.iter().any(|s| s.id == id) {
            return Err((StatusCode::NOT_FOUND, format!("Setlist {} not found", id)));
        }
    }

    *state.active_setlist_id.write().await = id.clone();

    save_active_setlist_to_disk(&state).await?;
    reload_audio_for_active_setlist(&state).await;

    let programs = state.programs.read().await;
    let mut setlist_programs: Vec<crate::program::Program> = programs
        .values()
        .filter(|p| p.setlist_id == id)
        .cloned()
        .collect();
    setlist_programs.sort_by_key(|p| p.display_order);

    info!("Activated setlist: {} ({} programs)", id, setlist_programs.len());
    Ok(Json(ActivateSetlistResponse {
        active_setlist_id: id,
        programs: setlist_programs,
    }))
}

async fn save_setlists_to_disk(
    state: &SharedState,
    setlists: &[Setlist],
) -> Result<(), (StatusCode, String)> {
    let active_id = state.active_setlist_id.read().await.clone();
    let store = SetlistStore {
        setlists: setlists.to_vec(),
        active_setlist_id: active_id,
    };
    store.save(&state.storage_paths.programs).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save setlists: {}", e))
    })
}

async fn save_active_setlist_to_disk(
    state: &SharedState,
) -> Result<(), (StatusCode, String)> {
    let setlists = state.setlists.read().await;
    save_setlists_to_disk(state, &setlists).await
}

pub async fn reload_audio_for_active_setlist(state: &SharedState) {
    let active_id = state.active_setlist_id.read().await.clone();

    {
        let mut engine = state.audio_engine.lock().await;
        engine.stop().await;
        engine.unload_all();
    }

    let programs = state.programs.read().await;
    let file_set = audio::collect_audio_files(&programs, &active_id);
    drop(programs);

    let device_sample_rate = {
        let engine = state.audio_engine.lock().await;
        engine.get_device_sample_rate()
    };
    audio::spawn_preload_filtered(
        state.audio_engine.clone(),
        state.storage_paths.audio.clone(),
        Some(file_set),
        Some(state.broadcast_tx.clone()),
        device_sample_rate,
    );
}
