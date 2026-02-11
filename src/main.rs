use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use http::Method;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

mod actor;
mod audio;
mod board;
mod config;
mod cue_scheduler;
mod effects;
mod effects_engine;
mod group;
mod pattern;
mod pattern_engine;
mod playback_history;
mod preset;
mod program;
mod program_engine;
mod routes;
mod sse;
mod timecode;
mod timing_metrics;
mod transport;
mod types;

use actor::BoardActor;
use config::Config;
use sse::SseEvent;
use types::{AppState, BoardEntry, SharedState};

#[tokio::main]
async fn main() {
    let startup_time = Instant::now();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Starting WLED Rust Server");

    let storage_paths = config::StoragePaths::default();

    if let Err(e) = storage_paths.init() {
        error!("Failed to initialize storage paths: {}", e);
        error!("Program storage will be unavailable");
    }

    let (broadcast_tx, _) = broadcast::channel::<SseEvent>(100);
    let broadcast_tx = Arc::new(broadcast_tx);

    let loaded_config = match Config::load() {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load config {}", e);
            error!("Fix data/boards.toml or delete it to start with defaults");
            return;
        }
    };
    let group_e131_transports = loaded_config.init_e131_transports().await;

    let timing_metrics = Arc::new(timing_metrics::TimingMetrics::new());
    let playback_history = Arc::new(playback_history::PlaybackHistory::new(
        storage_paths.history.clone(),
    ));
    let effects_engine = Arc::new(effects_engine::EffectsEngine::new(Some(
        timing_metrics.clone(),
    )));
    let pattern_engine = Arc::new(pattern_engine::PatternEngine::new());
    let device_manager = Arc::new(audio::DeviceManager::new());
    let startup_routing = loaded_config.audio.resolve_startup_routing(&device_manager);

    let mut audio_engine = audio::AudioEngine::new();
    audio_engine.set_device_sample_rate(device_manager.get_selected_sample_rate());
    audio_engine.set_resampling_quality(loaded_config.audio.resampling_quality);
    audio_engine.set_broadcast_tx(broadcast_tx.clone());

    let audio_thread = if let Some(command_rx) = audio_engine.take_receiver() {
        let position = audio_engine.get_position_arc();
        let health = audio_engine.get_health_arc();
        match audio::AudioThread::new(
            command_rx,
            position,
            health,
            device_manager.clone(),
            broadcast_tx.clone(),
        ) {
            Ok(thread) => {
                info!("Audio playback thread started");
                Some(Arc::new(thread))
            }
            Err(e) => {
                warn!(
                    "Failed to start audio thread: {} - audio playback disabled",
                    e
                );
                None
            }
        }
    } else {
        None
    };

    let audio_engine = Arc::new(Mutex::new(audio_engine));

    if let Some(routing) = startup_routing {
        let engine = audio_engine.lock().await;
        engine.update_routing(routing).await;
        info!("Applied startup routing to audio thread");
    }

    let programs_map: HashMap<String, program::Program> =
        match program::Program::load_all(&storage_paths.programs) {
            Ok(programs) => {
                info!("✅ Loaded {} program(s) into memory", programs.len());
                programs.into_iter().map(|p| (p.id.clone(), p)).collect()
            }
            Err(e) => {
                warn!("Failed to load programs: {} - starting with empty map", e);
                HashMap::new()
            }
        };
    let programs = Arc::new(RwLock::new(programs_map));

    let config_arc = Arc::new(Mutex::new(loaded_config.clone()));
    let performance_mode = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let loopy_ip = loaded_config.loopy_pro.ip.clone();
    let loopy_port = loaded_config.loopy_pro.port;

    let on_audio_play: program_engine::AudioPlayCallback = {
        let ip = loopy_ip.clone();
        let port = loopy_port;
        Arc::new(move |track: &str| {
            let address = format!("/Play/0:{}", track);
            if let Err(e) = routes::send_osc_sync(&ip, port, &address) {
                eprintln!("Failed to send OSC play: {}", e);
            }
        })
    };

    let connected_ips: Arc<RwLock<std::collections::HashSet<String>>> =
        Arc::new(RwLock::new(std::collections::HashSet::new()));

    let program_engine = Arc::new(program_engine::ProgramEngine::new(
        config_arc.clone(),
        effects_engine.clone(),
        pattern_engine.clone(),
        performance_mode.clone(),
        Some(on_audio_play),
        connected_ips.clone(),
        Some(timing_metrics.clone()),
        Some(playback_history.clone()),
        Some(audio_engine.clone()),
        broadcast_tx.clone(),
    ));

    let state: SharedState = Arc::new(AppState {
        boards: Arc::new(RwLock::new(HashMap::new())),
        broadcast_tx: broadcast_tx.clone(),
        storage_paths: Arc::new(storage_paths),
        group_e131_transports: Arc::new(RwLock::new(group_e131_transports)),
        config: config_arc,
        effects_engine,
        pattern_engine,
        device_manager,
        audio_engine,
        audio_thread,
        programs,
        program_engine,
        connected_ips: connected_ips.clone(),
        performance_mode: performance_mode.clone(),
        timing_metrics,
        playback_history,
        startup_time,
    });

    info!(
        "Loaded {} board(s) from boards.toml",
        loaded_config.boards.len()
    );
    for board_config in &loaded_config.boards {
        let (tx, rx) = mpsc::channel(100);
        {
            let mut senders = state.boards.write().await;
            senders.insert(
                board_config.id.clone(),
                BoardEntry {
                    ip: board_config.ip.clone(),
                    sender: tx,
                },
            );
        }
        let actor = BoardActor::new_with_config(
            board_config.id.clone(),
            board_config.ip.clone(),
            board_config.transition,
            board_config.led_count,
            board_config.universe,
            state.broadcast_tx.clone(),
            state.connected_ips.clone(),
            state.performance_mode.clone(),
            Some(startup_time),
            loaded_config.boards.len(),
        );
        tokio::spawn(async move {
            if let Err(e) = actor.run(rx).await {
                error!("Actor error: {}", e);
            }
        });
    }

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    let api_router = routes::build_api_router(state.clone());

    let frontend_path =
        std::env::var("WLED_FRONTEND_PATH").unwrap_or_else(|_| "frontend/build".to_string());

    let app = axum::Router::new()
        .nest("/api", api_router)
        .fallback_service(
            ServeDir::new(&frontend_path)
                .not_found_service(ServeFile::new(format!("{}/index.html", frontend_path))),
        )
        .layer(cors);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3010".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => {
            info!(
                "API Server running on http://{} (startup: {:?})",
                addr,
                startup_time.elapsed()
            );
            l
        }
        Err(e) => {
            error!("Failed to bind to {}: {}", addr, e);
            error!("Is port already in use?");
            return;
        }
    };

    async fn shutdown_signal() {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install ctrl+c handler");
        };

        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await;
        };

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    }

    audio::spawn_preload_task(
        state.audio_engine.clone(),
        state.storage_paths.audio.clone(),
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|e| error!("Server error: {}", e));

    info!("Server stopped, cleaning up...");

    state.program_engine.stop().await.ok();


    {
        let mut engine = state.audio_engine.lock().await;
        engine.stop().await;

    }

    info!("Shutdown complete");
}
