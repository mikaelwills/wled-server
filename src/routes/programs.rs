use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::Deserialize;
use tracing::{info, warn, error};

use crate::audio;
use crate::program;
use crate::types::SharedState;

pub async fn update_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(program): Json<program::Program>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    if id != program.id {
        return Err((StatusCode::BAD_REQUEST, "ID mismatch".to_string()));
    }

    program
        .save_to_file(&state.storage_paths.programs)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Failed to save program '{}' to {}: {}",
                    program.id,
                    state.storage_paths.programs.display(),
                    e
                ),
            )
        })?;

    {
        let mut programs = state.programs.write().await;
        programs.insert(program.id.clone(), program);
    }

    Ok(StatusCode::OK)
}

pub async fn get_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<program::Program>, (StatusCode, String)> {
    let programs = state.programs.read().await;
    let program = programs
        .get(&id)
        .cloned()
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;
    Ok(Json(program))
}

pub async fn delete_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let program = {
        let programs = state.programs.read().await;
        programs.get(&id).cloned()
    };

    let program = program.ok_or_else(|| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;

    {
        let mut engine = state.audio_engine.lock().await;

        if let Some(ref audio_file) = program.audio_file {
            let track_id = std::path::Path::new(audio_file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(audio_file);
            if engine.unload_track(track_id) {
                info!("Unloaded audio track from engine: {}", track_id);
            }
        }

        if let Some(ref guide_file) = program.guide_audio_file {
            let track_id = std::path::Path::new(guide_file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(guide_file);
            if engine.unload_guide_track(track_id) {
                info!("Unloaded guide audio track from engine: {}", track_id);
            }
        }
    }

    program
        .delete(&state.storage_paths.programs, &state.storage_paths.audio)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete program '{}': {}", id, e),
            )
        })?;

    {
        let mut programs = state.programs.write().await;
        programs.remove(&id);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn save_program(
    State(state): State<SharedState>,
    Json(program): Json<program::Program>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    program
        .save_to_file(&state.storage_paths.programs)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    {
        let mut programs = state.programs.write().await;
        programs.insert(program.id.clone(), program);
    }

    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
pub struct ListProgramsQuery {
    #[serde(default)]
    pub setlist_id: Option<String>,
}

pub async fn list_programs(
    State(state): State<SharedState>,
    axum::extract::Query(query): axum::extract::Query<ListProgramsQuery>,
) -> Json<Vec<program::Program>> {
    let programs = state.programs.read().await;
    let mut list: Vec<program::Program> = programs
        .values()
        .filter(|p| {
            if let Some(ref sid) = query.setlist_id {
                &p.setlist_id == sid
            } else {
                true
            }
        })
        .cloned()
        .collect();
    list.sort_by_key(|p| p.display_order);
    Json(list)
}

#[derive(serde::Deserialize)]
pub struct PlayProgramRequest {
    #[serde(default)]
    start: f64,
}

pub async fn play_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<PlayProgramRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let program = {
        let programs = state.programs.read().await;
        programs.get(&id).cloned()
    };

    let program = program.ok_or_else(|| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;

    state
        .program_engine
        .play(program, params.start)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::OK)
}

pub async fn stop_program(
    State(state): State<SharedState>,
) -> Result<StatusCode, (StatusCode, String)> {
    println!("🛑🛑🛑 API: /programs/stop called - MANUAL STOP FROM FRONTEND 🛑🛑🛑");

    match state.program_engine.stop().await {
        Ok(_) => {
            println!("🛑 API: stop() succeeded");
            info!("⏹️ Stopped program playback");
            Ok(StatusCode::OK)
        }
        Err(e) => {
            println!("🛑 API: stop() failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e))
        }
    }
}

#[derive(serde::Serialize)]
pub struct ReloadResponse {
    pub loaded: usize,
    pub previous: usize,
}

pub async fn reload_programs(
    State(state): State<SharedState>,
) -> Result<Json<ReloadResponse>, (StatusCode, String)> {
    let previous = {
        let programs = state.programs.read().await;
        programs.len()
    };

    info!("🔄 Reloading programs from disk (previous count: {})", previous);

    let new_programs = program::Program::load_all(&state.storage_paths.programs)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load programs: {}", e),
            )
        })?;

    let loaded = new_programs.len();

    {
        let mut programs = state.programs.write().await;
        programs.clear();
        for p in new_programs {
            programs.insert(p.id.clone(), p);
        }
    }

    info!("✅ Reloaded {} program(s) from disk", loaded);

    Ok(Json(ReloadResponse { loaded, previous }))
}

fn copy_file_if_exists(src: &std::path::Path, dst: &std::path::Path, label: &str) {
    if src.exists() {
        match std::fs::copy(src, dst) {
            Ok(bytes) => {
                info!("Copied {} ({} bytes): {} -> {}", label, bytes, src.display(), dst.display());
                if let Ok(src_meta) = std::fs::metadata(src) {
                    if let Ok(mtime) = src_meta.modified() {
                        if let Ok(dst_file) = std::fs::File::options().write(true).open(dst) {
                            let _ = dst_file.set_modified(mtime);
                        }
                    }
                }
            }
            Err(e) => warn!("Failed to copy {}: {} -> {}: {}", label, src.display(), dst.display(), e),
        }
    } else {
        warn!("Source {} not found, skipping: {}", label, src.display());
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

pub async fn duplicate_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<program::Program>), (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let (source, max_order) = {
        let programs = state.programs.read().await;
        let source = programs
            .get(&id)
            .cloned()
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;
        let max_order = programs.values().map(|p| p.display_order).max().unwrap_or(0);
        (source, max_order)
    };

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let new_song_name = format!("{} (Copy)", source.song_name);

    let sanitized: String = new_song_name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let collapsed = sanitized
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let new_id = format!("{}-{}", collapsed, timestamp);

    let mut clone = source.clone();
    clone.id = new_id.clone();
    clone.song_name = new_song_name;
    clone.created_at = chrono::Utc::now().to_rfc3339();
    clone.next_program_id = None;
    clone.display_name = None;
    clone.display_order = max_order + 1;

    let audio_dir = &state.storage_paths.audio;
    let resampled_dir = audio_dir.join("resampled");

    if let Some(ref old_audio_file) = source.audio_file {
        let ext = std::path::Path::new(old_audio_file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp3");
        let new_audio_file = format!("{}.{}", new_id, ext);

        copy_file_if_exists(
            &audio_dir.join(old_audio_file),
            &audio_dir.join(&new_audio_file),
            "backing audio",
        );
        copy_file_if_exists(
            &audio_dir.join(format!("{}.peaks.json", old_audio_file)),
            &audio_dir.join(format!("{}.peaks.json", new_audio_file)),
            "backing peaks",
        );

        let old_cache_dir = resampled_dir.join(old_audio_file);
        if old_cache_dir.exists() {
            let new_cache_dir = resampled_dir.join(&new_audio_file);
            match copy_dir_recursive(&old_cache_dir, &new_cache_dir) {
                Ok(_) => info!("Copied resampled cache: {} -> {}", old_cache_dir.display(), new_cache_dir.display()),
                Err(e) => warn!("Failed to copy resampled cache: {}", e),
            }
        }

        clone.audio_file = Some(new_audio_file);
    }

    if let Some(ref old_guide_file) = source.guide_audio_file {
        let ext = std::path::Path::new(old_guide_file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp3");
        let new_guide_file = format!("{}_guide.{}", new_id, ext);

        copy_file_if_exists(
            &audio_dir.join(old_guide_file),
            &audio_dir.join(&new_guide_file),
            "guide audio",
        );
        copy_file_if_exists(
            &audio_dir.join(format!("{}.peaks.json", old_guide_file)),
            &audio_dir.join(format!("{}.peaks.json", new_guide_file)),
            "guide peaks",
        );

        let old_cache_dir = resampled_dir.join(old_guide_file);
        if old_cache_dir.exists() {
            let new_cache_dir = resampled_dir.join(&new_guide_file);
            match copy_dir_recursive(&old_cache_dir, &new_cache_dir) {
                Ok(_) => info!("Copied guide resampled cache: {} -> {}", old_cache_dir.display(), new_cache_dir.display()),
                Err(e) => warn!("Failed to copy guide resampled cache: {}", e),
            }
        }

        clone.guide_audio_file = Some(new_guide_file);
    }

    clone
        .save_to_file(&state.storage_paths.programs)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to save duplicated program: {}", e),
            )
        })?;

    info!("Duplicated program '{}' -> '{}' ({})", source.id, clone.id, clone.song_name);

    let cache_dir = audio_dir.join("resampled");
    if let Some(ref new_audio_file) = clone.audio_file {
        let audio_path = audio_dir.join(new_audio_file);
        let audio_engine = state.audio_engine.clone();
        let track_id = std::path::Path::new(new_audio_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(new_audio_file)
            .to_string();
        let cache_dir_clone = cache_dir.clone();
        tokio::spawn(async move {
            let path = audio_path;
            let decode_result = tokio::task::spawn_blocking(move || audio::decode_file_with_path(&path)).await;
            match decode_result {
                Ok(Ok(decoded)) => {
                    let track = decoded.track.with_source_info(decoded.source_path, cache_dir_clone);
                    let mut engine = audio_engine.lock().await;
                    engine.load_track(track_id.clone(), track).await;
                    info!("Loaded duplicated backing track into engine: {}", track_id);
                }
                Ok(Err(e)) => error!("Failed to decode duplicated backing audio: {}", e),
                Err(e) => error!("Decode task failed for duplicated backing: {}", e),
            }
        });
    }

    if let Some(ref new_guide_file) = clone.guide_audio_file {
        let guide_path = audio_dir.join(new_guide_file);
        let audio_engine = state.audio_engine.clone();
        let track_id = std::path::Path::new(new_guide_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(new_guide_file)
            .to_string();
        let cache_dir_clone = cache_dir.clone();
        tokio::spawn(async move {
            let path = guide_path;
            let decode_result = tokio::task::spawn_blocking(move || audio::decode_file_with_path(&path)).await;
            match decode_result {
                Ok(Ok(decoded)) => {
                    let track = decoded.track.with_source_info(decoded.source_path, cache_dir_clone);
                    let mut engine = audio_engine.lock().await;
                    engine.load_guide_track(track_id.clone(), track).await;
                    info!("Loaded duplicated guide track into engine: {}", track_id);
                }
                Ok(Err(e)) => error!("Failed to decode duplicated guide audio: {}", e),
                Err(e) => error!("Decode task failed for duplicated guide: {}", e),
            }
        });
    }

    let result = clone.clone();
    {
        let mut programs = state.programs.write().await;
        programs.insert(clone.id.clone(), clone);
    }

    Ok((StatusCode::CREATED, Json(result)))
}

#[derive(Deserialize)]
pub struct ReplaceAudioRequest {
    pub data_url: String,
}

pub async fn replace_audio(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(payload): Json<ReplaceAudioRequest>,
) -> Result<Json<program::Program>, (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Storage not available".to_string(),
        ));
    }

    let program = {
        let programs = state.programs.read().await;
        programs
            .get(&id)
            .cloned()
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?
    };

    if let Some(ref old_audio_file) = program.audio_file {
        let track_id = std::path::Path::new(old_audio_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(old_audio_file);
        let mut engine = state.audio_engine.lock().await;
        if engine.unload_track(track_id) {
            info!("Unloaded old backing track from engine: {}", track_id);
        }
    }

    let audio_dir = &state.storage_paths.audio;
    if let Some(ref old_audio_file) = program.audio_file {
        let old_path = audio_dir.join(old_audio_file);
        if old_path.exists() {
            let _ = std::fs::remove_file(&old_path);
            info!("Deleted old audio file: {}", old_audio_file);
        }
        let old_peaks = audio_dir.join(format!("{}.peaks.json", old_audio_file));
        if old_peaks.exists() {
            let _ = std::fs::remove_file(&old_peaks);
            info!("Deleted old peaks: {}.peaks.json", old_audio_file);
        }
        let old_cache_dir = audio_dir.join("resampled").join(old_audio_file);
        if old_cache_dir.exists() {
            let _ = std::fs::remove_dir_all(&old_cache_dir);
            info!("Deleted old resampled cache: resampled/{}", old_audio_file);
        }
    }

    let new_filename = audio::AudioFile::save(&program.id, &payload.data_url, audio_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save new audio: {}", e)))?;

    info!("Saved new audio file: {}", new_filename);

    let mut updated = program.clone();
    updated.audio_file = Some(new_filename.clone());
    updated.audio_duration = None;

    updated
        .save_to_file(&state.storage_paths.programs)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to save program: {}", e),
            )
        })?;

    let audio_path = audio_dir.join(&new_filename);
    let cache_dir = audio_dir.join("resampled");
    let audio_engine = state.audio_engine.clone();
    let track_id = std::path::Path::new(&new_filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&new_filename)
        .to_string();
    tokio::spawn(async move {
        let _ = std::fs::create_dir_all(&cache_dir);
        let cache_dir_clone = cache_dir.clone();
        let decode_result = tokio::task::spawn_blocking(move || audio::decode_file_with_path(&audio_path)).await;
        match decode_result {
            Ok(Ok(decoded)) => {
                let track = decoded.track.with_source_info(decoded.source_path, cache_dir_clone);
                let mut engine = audio_engine.lock().await;
                engine.load_track(track_id.clone(), track).await;
                info!("Loaded replacement backing track into engine: {}", track_id);
            }
            Ok(Err(e)) => error!("Failed to decode replacement audio: {}", e),
            Err(e) => error!("Decode task failed for replacement: {}", e),
        }
    });

    let result = updated.clone();
    {
        let mut programs = state.programs.write().await;
        programs.insert(updated.id.clone(), updated);
    }

    info!("Replaced audio for program '{}' with '{}'", id, new_filename);
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct MoveProgramRequest {
    pub setlist_id: String,
}

pub async fn move_program(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(payload): Json<MoveProgramRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let active_id = state.active_setlist_id.read().await.clone();

    let old_setlist_id = {
        let mut programs = state.programs.write().await;
        let program = programs
            .get_mut(&id)
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;
        let old = program.setlist_id.clone();
        program.setlist_id = payload.setlist_id.clone();
        program
            .save_to_file(&state.storage_paths.programs)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save: {}", e)))?;
        old
    };

    let program = {
        let programs = state.programs.read().await;
        programs.get(&id).cloned()
    };

    if let Some(ref prog) = program {
        if old_setlist_id == active_id && payload.setlist_id != active_id {
            let mut engine = state.audio_engine.lock().await;
            unload_program_tracks(&mut engine, prog);
        } else if old_setlist_id != active_id && payload.setlist_id == active_id {
            load_program_tracks(&state, prog).await;
        }
    }

    info!("Moved program {} from setlist {} to {}", id, old_setlist_id, payload.setlist_id);
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct CloneToSetlistRequest {
    pub setlist_id: String,
}

pub async fn clone_to_setlist(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(payload): Json<CloneToSetlistRequest>,
) -> Result<(StatusCode, Json<program::Program>), (StatusCode, String)> {
    if !state.storage_paths.is_available() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "Storage not available".to_string()));
    }

    let (source, max_order) = {
        let programs = state.programs.read().await;
        let source = programs
            .get(&id)
            .cloned()
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Program {} not found", id)))?;
        let max_order = programs.values().map(|p| p.display_order).max().unwrap_or(0);
        (source, max_order)
    };

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let new_song_name = format!("{} (Copy)", source.song_name);
    let sanitized: String = new_song_name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let collapsed = sanitized
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let new_id = format!("{}-{}", collapsed, timestamp);

    let mut clone = source.clone();
    clone.id = new_id.clone();
    clone.song_name = new_song_name;
    clone.created_at = chrono::Utc::now().to_rfc3339();
    clone.next_program_id = None;
    clone.display_order = max_order + 1;
    clone.setlist_id = payload.setlist_id.clone();

    let audio_dir = &state.storage_paths.audio;
    let resampled_dir = audio_dir.join("resampled");

    if let Some(ref old_audio_file) = source.audio_file {
        let ext = std::path::Path::new(old_audio_file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp3");
        let new_audio_file = format!("{}.{}", new_id, ext);
        copy_file_if_exists(&audio_dir.join(old_audio_file), &audio_dir.join(&new_audio_file), "backing audio");
        copy_file_if_exists(
            &audio_dir.join(format!("{}.peaks.json", old_audio_file)),
            &audio_dir.join(format!("{}.peaks.json", new_audio_file)),
            "backing peaks",
        );
        let old_cache_dir = resampled_dir.join(old_audio_file);
        if old_cache_dir.exists() {
            let new_cache_dir = resampled_dir.join(&new_audio_file);
            match copy_dir_recursive(&old_cache_dir, &new_cache_dir) {
                Ok(_) => info!("Copied resampled cache for clone"),
                Err(e) => warn!("Failed to copy resampled cache: {}", e),
            }
        }
        clone.audio_file = Some(new_audio_file);
    }

    if let Some(ref old_guide_file) = source.guide_audio_file {
        let ext = std::path::Path::new(old_guide_file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp3");
        let new_guide_file = format!("{}_guide.{}", new_id, ext);
        copy_file_if_exists(&audio_dir.join(old_guide_file), &audio_dir.join(&new_guide_file), "guide audio");
        copy_file_if_exists(
            &audio_dir.join(format!("{}.peaks.json", old_guide_file)),
            &audio_dir.join(format!("{}.peaks.json", new_guide_file)),
            "guide peaks",
        );
        let old_cache_dir = resampled_dir.join(old_guide_file);
        if old_cache_dir.exists() {
            let new_cache_dir = resampled_dir.join(&new_guide_file);
            match copy_dir_recursive(&old_cache_dir, &new_cache_dir) {
                Ok(_) => info!("Copied guide resampled cache for clone"),
                Err(e) => warn!("Failed to copy guide resampled cache: {}", e),
            }
        }
        clone.guide_audio_file = Some(new_guide_file);
    }

    clone
        .save_to_file(&state.storage_paths.programs)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save cloned program: {}", e)))?;

    let active_id = state.active_setlist_id.read().await.clone();
    if payload.setlist_id == active_id {
        load_program_tracks(&state, &clone).await;
    }

    info!("Cloned program '{}' -> '{}' into setlist '{}'", source.id, clone.id, payload.setlist_id);

    let result = clone.clone();
    {
        let mut programs = state.programs.write().await;
        programs.insert(clone.id.clone(), clone);
    }

    Ok((StatusCode::CREATED, Json(result)))
}

fn unload_program_tracks(engine: &mut crate::audio::AudioEngine, program: &program::Program) {
    if let Some(ref audio_file) = program.audio_file {
        let track_id = std::path::Path::new(audio_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(audio_file);
        if engine.unload_track(track_id) {
            info!("Unloaded backing track: {}", track_id);
        }
    }
    if let Some(ref guide_file) = program.guide_audio_file {
        let track_id = std::path::Path::new(guide_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(guide_file);
        if engine.unload_guide_track(track_id) {
            info!("Unloaded guide track: {}", track_id);
        }
    }
}

async fn load_program_tracks(state: &SharedState, program: &program::Program) {
    let audio_dir = &state.storage_paths.audio;
    let cache_dir = audio_dir.join("resampled");

    if let Some(ref audio_file) = program.audio_file {
        let audio_path = audio_dir.join(audio_file);
        let audio_engine = state.audio_engine.clone();
        let track_id = std::path::Path::new(audio_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(audio_file)
            .to_string();
        let cache_dir_clone = cache_dir.clone();
        tokio::spawn(async move {
            let path = audio_path;
            let decode_result = tokio::task::spawn_blocking(move || audio::decode_file_with_path(&path)).await;
            match decode_result {
                Ok(Ok(decoded)) => {
                    let track = decoded.track.with_source_info(decoded.source_path, cache_dir_clone);
                    let mut engine = audio_engine.lock().await;
                    engine.load_track(track_id.clone(), track).await;
                    info!("Loaded backing track into engine: {}", track_id);
                }
                Ok(Err(e)) => error!("Failed to decode backing audio: {}", e),
                Err(e) => error!("Decode task failed for backing: {}", e),
            }
        });
    }

    if let Some(ref guide_file) = program.guide_audio_file {
        let guide_path = audio_dir.join(guide_file);
        let audio_engine = state.audio_engine.clone();
        let track_id = std::path::Path::new(guide_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(guide_file)
            .to_string();
        let cache_dir_clone = cache_dir.clone();
        tokio::spawn(async move {
            let path = guide_path;
            let decode_result = tokio::task::spawn_blocking(move || audio::decode_file_with_path(&path)).await;
            match decode_result {
                Ok(Ok(decoded)) => {
                    let track = decoded.track.with_source_info(decoded.source_path, cache_dir_clone);
                    let mut engine = audio_engine.lock().await;
                    engine.load_guide_track(track_id.clone(), track).await;
                    info!("Loaded guide track into engine: {}", track_id);
                }
                Ok(Err(e)) => error!("Failed to decode guide audio: {}", e),
                Err(e) => error!("Decode task failed for guide: {}", e),
            }
        });
    }
}
