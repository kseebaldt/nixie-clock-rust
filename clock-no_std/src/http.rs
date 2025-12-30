//! HTTP server module using picoserve

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::watch::Sender;
use embassy_time::Duration;
use esp_println::println;

use drivers::config::{Config, InternalConfig};
use picoserve::AppWithStateBuilder;
use picoserve::extract::State;
use picoserve::response::{IntoResponse, StatusCode};
use picoserve::routing::get;

use crate::storage::SeqConfigStorage;

/// Embedded webapp HTML
pub static INDEX_HTML: &str = include_str!("../../webapp/dist/index.html");

/// Application state for HTTP handlers
pub struct AppState {
    pub config_storage: &'static Mutex<CriticalSectionRawMutex, SeqConfigStorage<'static>>,
    pub config_sender: Sender<'static, CriticalSectionRawMutex, InternalConfig, 3>,
    pub current_config: &'static Mutex<CriticalSectionRawMutex, InternalConfig>,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            config_storage: self.config_storage,
            config_sender: self.config_sender.clone(),
            current_config: self.current_config,
        }
    }
}

/// App builder properties - used to create the router
pub struct AppProps;

impl AppWithStateBuilder for AppProps {
    type State = AppState;
    type PathRouter = impl picoserve::routing::PathRouter<AppState>;

    fn build_app(self) -> picoserve::Router<Self::PathRouter, AppState> {
        picoserve::Router::new()
            .route("/", get(get_index))
            .route("/config", get(get_config).post(post_config))
    }
}

/// Server configuration
pub fn server_config() -> picoserve::Config<Duration> {
    picoserve::Config::new(picoserve::Timeouts {
        start_read_request: Some(Duration::from_secs(5)),
        persistent_start_read_request: Some(Duration::from_secs(1)),
        read_request: Some(Duration::from_secs(1)),
        write: Some(Duration::from_secs(1)),
    })
}

/// GET / - Serve the webapp HTML
async fn get_index() -> impl IntoResponse {
    (StatusCode::OK, [("Content-Type", "text/html")], INDEX_HTML)
}

/// GET /config - Return current config as JSON
async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    let current_config = state.current_config.lock().await;
    let json_config: Config = (*current_config).clone().into();
    drop(current_config);

    picoserve::response::Json(json_config)
}

/// POST /config - Update config
async fn post_config(
    State(state): State<AppState>,
    picoserve::extract::Json(new_config): picoserve::extract::Json<Config>,
) -> impl IntoResponse {
    // Validate config
    if let Err(e) = new_config.validate() {
        println!("HTTP: Config validation error: {} - {}", e.field, e.message);
        return (StatusCode::BAD_REQUEST, "{\"error\":\"validation failed\"}");
    }

    // Convert to internal config
    let internal_config: InternalConfig = new_config.into();

    // Serialize and save to flash
    let mut serialized_buf = [0u8; 256];
    let serialized = match postcard::to_slice(&internal_config, &mut serialized_buf) {
        Ok(v) => v,
        Err(e) => {
            println!("HTTP: Failed to serialize config: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "{\"error\":\"serialization failed\"}",
            );
        }
    };

    {
        let mut storage = state.config_storage.lock().await;
        if let Err(e) = storage.save(serialized).await {
            println!("HTTP: Failed to save config: {:?}", e);
        } else {
            println!("HTTP: Config saved to flash");
        }
    }

    // Update current config mutex
    {
        let mut cfg = state.current_config.lock().await;
        *cfg = internal_config.clone();
    }

    // Broadcast to all watchers (RGB, WiFi tasks)
    state.config_sender.send(internal_config);
    println!("HTTP: Config updated and broadcast");

    (StatusCode::OK, "{\"status\":\"ok\"}")
}
