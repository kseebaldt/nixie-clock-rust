# Picoserve Implementation for Nixie Clock

## Overview
Picoserve can handle all the nixie clock's web server requirements with its no_std async architecture.

## Dependencies
```toml
[dependencies]
picoserve = "0.10"
serde = { version = "1.0", default-features = false, features = ["derive"] }
serde_json = { version = "1.0", default-features = false }
heapless = "0.8"
```

## Implementation Example

```rust
use picoserve::{
    extract::{FromRequest, Json, State},
    response::{IntoResponse, Json as JsonResponse, Response, StatusCode},
    routing::{get, post, RequestParts},
    Router,
};
use serde::{Deserialize, Serialize};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};

// Your existing Config struct
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub time_zone: String,
    pub led_color: String,
    pub hours24: bool,
}

// Response types
#[derive(Serialize)]
struct StatusResponse {
    status: &'static str,
}

// Shared state for the web server
pub struct AppState {
    pub config_channel: &'static Channel<CriticalSectionRawMutex, Config, 1>,
    pub storage: &'static Mutex<CriticalSectionRawMutex, ConfigStorage>,
}

// Handler functions
async fn serve_index() -> impl IntoResponse {
    // Embedded HTML from build
    const INDEX_HTML: &str = include_str!("../webapp/dist/index.html");
    
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(INDEX_HTML)
}

async fn get_config(State(state): State<&AppState>) -> impl IntoResponse {
    let storage = state.storage.lock().await;
    match storage.load_config().await {
        Ok(config) => JsonResponse(config).into_response(),
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body("Failed to load config")
            .into_response(),
    }
}

async fn update_config(
    State(state): State<&AppState>,
    Json(new_config): Json<Config>,
) -> impl IntoResponse {
    // Validate config
    if new_config.wifi_ssid.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("WiFi SSID cannot be empty")
            .into_response();
    }
    
    if new_config.time_zone.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("Timezone cannot be empty")
            .into_response();
    }
    
    // Validate hex color format
    if !new_config.led_color.starts_with('#') || new_config.led_color.len() != 7 {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("Invalid color format")
            .into_response();
    }
    
    // Save to storage
    let mut storage = state.storage.lock().await;
    match storage.save_config(&new_config).await {
        Ok(_) => {
            // Send to channel for hot reload
            let _ = state.config_channel.try_send(new_config);
            JsonResponse(StatusResponse { status: "ok" }).into_response()
        }
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body("Failed to save config")
            .into_response(),
    }
}

// Create the router
pub fn create_app(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(serve_index))
        .route("/config", get(get_config).post(update_config))
        .with_state(state)
}

// Server task for Embassy
#[embassy_executor::task]
pub async fn web_server_task(
    stack: &'static Stack<WifiDevice>,
    config_channel: &'static Channel<CriticalSectionRawMutex, Config, 1>,
    storage: &'static Mutex<CriticalSectionRawMutex, ConfigStorage>,
) {
    let state = AppState {
        config_channel,
        storage,
    };
    
    let app = create_app(state);
    let config = picoserve::Config::new(picoserve::Timeouts {
        start_read_request: Some(Duration::from_secs(5)),
        read_request: Some(Duration::from_secs(1)),
        write: Some(Duration::from_secs(1)),
    });
    
    loop {
        // Accept TCP connection
        let mut rx_buffer = [0; 2048];
        let mut tx_buffer = [0; 2048];
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        
        socket.bind(80).unwrap();
        socket.listen(1).unwrap();
        
        if let Ok(socket) = socket.accept().await {
            match picoserve::serve(&app, &config, &mut [0; 2048], socket).await {
                Ok(handled) => info!("Handled {} requests", handled),
                Err(e) => warn!("Connection error: {:?}", e),
            }
        }
    }
}
```

## Key Features Coverage

✅ **GET /** - Serves embedded HTML  
✅ **GET /config** - Returns JSON configuration  
✅ **POST /config** - Updates configuration with validation  
✅ **JSON handling** - Built-in with serde  
✅ **Validation** - Manual validation in handlers  
✅ **Hot reload** - Via channel communication  
✅ **No heap usage** - Perfect for embedded  

## Advantages over Raw TCP Sockets

1. **Cleaner API** - Route definition is much more readable
2. **Automatic parsing** - JSON extraction handled by framework
3. **Type safety** - Compile-time route checking
4. **Less boilerplate** - No manual HTTP parsing
5. **Extensible** - Easy to add new routes

## Migration Notes

1. **Validation**: Replace `validator` crate with manual validation
2. **Error responses**: Use picoserve's response builder
3. **State management**: Use picoserve's State extractor
4. **Content types**: Set manually in response builder
5. **Request size**: Configure in picoserve timeouts

## Limitations

- URL-encoded data limited to 1024 chars (not an issue for your use case)
- No built-in validation framework (need manual checks)
- No middleware system (but you don't need it)

## Conclusion

Picoserve is **perfect** for the nixie clock's web server needs. It provides all required functionality with a clean API and no_std support.