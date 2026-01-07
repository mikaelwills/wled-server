use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use http::Method;
use tower_http::cors::CorsLayer;

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
mod preset;
mod program;
mod program_engine;
mod routes;
mod sse;
mod transport;
mod types;

use actor::BoardActor;
use config::Config;
use sse::SseEvent;
use types::{AppState, BoardEntry, SharedState};

#[tokio::main]
async fn main() {
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

    let loaded_config = Config::load().unwrap_or(Config {
        boards: vec![],
        groups: vec![],
        loopy_pro: config::LoopyProConfig::default(),
        effect_presets: vec![],
        pattern_presets: vec![],
    });

    let mut group_e131_transports = HashMap::new();

    for (universe_index, group) in loaded_config.groups.iter().enumerate() {
        let mut group_board_ips: Vec<String> = Vec::new();
        for member_id in &group.members {
            if let Some(board) = loaded_config.boards.iter().find(|b| &b.id == member_id) {
                if !group_board_ips.contains(&board.ip) {
                    group_board_ips.push(board.ip.clone());
                }
            }
        }

        if !group_board_ips.is_empty() {
            let universe = group.universe.unwrap_or((universe_index + 1) as u16);

            info!(
                group_id = %group.id,
                universe = universe,
                board_count = group_board_ips.len(),
                "Initializing E1.31 transport for group: {:?}",
                group_board_ips
            );

            match transport::E131RawTransport::new(group_board_ips, universe) {
                Ok(transport) => {
                    group_e131_transports.insert(group.id.clone(), transport);
                    info!(group_id = %group.id, universe = universe, "E1.31 transport initialized");
                }
                Err(e) => {
                    warn!(group_id = %group.id, "Failed to initialize E1.31 transport: {}", e);
                }
            }
        } else {
            warn!(group_id = %group.id, "No boards found for group - will use WebSocket only");
        }
    }

    info!("Initialized {} E1.31 group transport(s)", group_e131_transports.len());

    info!("Configuring board E1.31 universes in parallel...");
    let mut config_tasks = Vec::new();

    for board in &loaded_config.boards {
        if let Some(universe) = board.universe {
            let board_id = board.id.clone();
            let board_ip = board.ip.clone();

            let task = tokio::spawn(async move {
                info!(
                    board_id = %board_id,
                    universe = universe,
                    "Configuring board universe"
                );

                match routes::groups::configure_board_universe(&board_ip, universe).await {
                    Ok(()) => {
                        info!(board_id = %board_id, universe = universe, "Successfully configured universe");
                    }
                    Err(e) => {
                        warn!(
                            board_id = %board_id,
                            universe = universe,
                            "Failed to configure universe: {}. Board may need manual configuration.", e
                        );
                    }
                }
            });

            config_tasks.push(task);
        }
    }

    let config_timeout = tokio::time::Duration::from_secs(10);
    match tokio::time::timeout(config_timeout, futures::future::join_all(config_tasks)).await {
        Ok(_) => info!("Universe configuration complete"),
        Err(_) => warn!("Universe configuration timed out after 10s - some boards may not be configured"),
    }

    let effects_engine = Arc::new(effects_engine::EffectsEngine::new());
    let pattern_engine = Arc::new(pattern_engine::PatternEngine::new());

    let programs_map: HashMap<String, program::Program> =
        match program::Program::load_all(&storage_paths.programs) {
            Ok(programs) => {
                info!("Loaded {} program(s) into memory", programs.len());
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

    let on_audio_stop: program_engine::AudioStopCallback = {
        let ip = loopy_ip;
        let port = loopy_port;
        Arc::new(move |track: &str| {
            let address = format!("/Stop/0:{}", track);
            if let Err(e) = routes::send_osc_sync(&ip, port, &address) {
                eprintln!("Failed to send OSC stop: {}", e);
            }
        })
    };

    let connected_ips: Arc<RwLock<std::collections::HashSet<String>>> = Arc::new(RwLock::new(std::collections::HashSet::new()));

    let program_engine = Arc::new(program_engine::ProgramEngine::new(
        config_arc.clone(),
        effects_engine.clone(),
        pattern_engine.clone(),
        performance_mode.clone(),
        Some(on_audio_play),
        Some(on_audio_stop),
        connected_ips.clone(),
    ));

    let state: SharedState = Arc::new(AppState {
        boards: Arc::new(RwLock::new(HashMap::new())),
        broadcast_tx: Arc::new(broadcast_tx),
        storage_paths: Arc::new(storage_paths),
        group_e131_transports: Arc::new(RwLock::new(group_e131_transports)),
        config: config_arc,
        effects_engine,
        pattern_engine,
        programs,
        program_engine,
        connected_ips: connected_ips.clone(),
        performance_mode: performance_mode.clone(),
    });

    match Config::load() {
        Ok(config) => {
            info!("Loaded {} board(s) from boards.toml", config.boards.len());
            for board_config in config.boards {
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
                );
                tokio::spawn(async move {
                    if let Err(e) = actor.run(rx).await {
                        error!("Actor error: {}", e);
                    }
                });
            }
        }
        Err(e) => {
            warn!("Could not load boards.toml: {}", e);
            info!("Server starting with no boards configured");
        }
    }

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    let api_router = routes::build_api_router(state.clone());

    let app = axum::Router::new().nest("/api", api_router).layer(cors);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3010".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => {
            info!("API Server running on http://{}", addr);
            l
        }
        Err(e) => {
            error!("Failed to bind to {}: {}", addr, e);
            error!("Is port already in use?");
            return;
        }
    };

    match axum::serve(listener, app).await {
        Ok(_) => info!("Server stopped properly"),
        Err(e) => error!("Server error: {}", e),
    }
}
