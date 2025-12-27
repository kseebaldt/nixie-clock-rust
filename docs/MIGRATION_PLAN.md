# ESP-HAL v1.0 Migration Plan

## Overview

Migrate the nixie clock from **esp-idf-svc** (`clock/`) to **esp-hal v1.0 + Embassy** (`clock-no_std/`).

| Aspect | Old (clock/) | New (clock-no_std/) |
|--------|--------------|---------------------|
| Runtime | `std` + FreeRTOS | `no_std` + Embassy async |
| WiFi | `esp-idf-svc::wifi` | `esp-radio::wifi` |
| HTTP Server | `EspHttpServer` | `picoserve` |
| Time Sync | `EspSntp` | `sntpc` |
| Storage | ESP NVS | `esp-storage` (raw flash) |
| Logging | `log` crate | `defmt` |
| PWM/LED | `esp-idf-hal::ledc` | `esp-hal::ledc` |
| Strings | `std::string::String` | `alloc::string::String` |
| Sync primitives | `std::sync::Mutex` | `embassy_sync::mutex::Mutex` |

---

## Decisions Made

- **Drivers crate**: Use `alloc` (keep `String`, `Box`, `Vec`) - simpler since heap is already available
- **Validation**: Manual validation (replace `validator` crate which requires std)
- **Shared drivers**: Make `drivers/` crate `no_std` compatible, shared between both projects
- **Logging**: `defmt` for all error handling and debugging
- **Config notification**: Use `embassy_sync::signal::Signal` to notify main task of config changes
- **Display update**: 200ms interval (same as original)

---

## Phase 0: Prepare Drivers Crate for no_std

**Goal**: Make `drivers/` crate work in both std and no_std environments.

### Changes to `drivers/Cargo.toml`

```toml
[package]
name = "drivers"
version = "0.1.0"
edition = "2021"

[features]
default = ["std"]
std = []

[dependencies]
embedded-hal = "1.0.0"
serde = { version = "1.0", default-features = false, features = ["derive", "alloc"] }
chrono = { version = "0.4", default-features = false }
postcard = { version = "1.1", default-features = false, features = ["alloc"] }
hashbrown = "0.15"  # no_std HashMap replacement

[dev-dependencies]
testing = { path = "../testing" }
```

**Remove**: `thiserror`, `validator`, `toml-cfg` (all require std)

### Changes to `drivers/src/lib.rs`

```rust
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
extern crate embedded_hal as hal;

pub mod config;
pub mod debouncer;
pub mod nixie_display;
pub mod rgb_led;
pub mod shift_register;
pub mod storage;
```

### Changes to `drivers/src/storage.rs`

- Replace `use std::collections::HashMap` with `use hashbrown::HashMap`
- Replace `use std::cmp::Ordering` with `use core::cmp::Ordering`
- Replace `thiserror::Error` with manual `Debug` implementation:

```rust
#[derive(Debug, Clone, Copy)]
pub enum StorageError {
    ReadError,
    WriteError,
}
```

### Changes to `drivers/src/config.rs`

- Add alloc imports:
  ```rust
  use alloc::boxed::Box;
  use alloc::string::String;
  use alloc::format;
  ```
- Remove `validator` derive macros
- Remove `toml_cfg` macro, replace with constants:
  ```rust
  pub const DEFAULT_WIFI_SSID: &str = "Wokwi-GUEST";
  pub const DEFAULT_WIFI_PASS: &str = "";
  pub const DEFAULT_AP_SSID: &str = "nixie-clock";
  pub const DEFAULT_AP_PASS: &str = "";
  ```
- Add manual validation function:
  ```rust
  impl Config {
      pub fn validate(&self) -> Result<(), &'static str> {
          if self.wifi_ssid.is_empty() {
              return Err("SSID must not be blank");
          }
          if self.time_zone.is_empty() {
              return Err("time zone must not be blank");
          }
          if !self.led_color.starts_with('#') || self.led_color.len() != 7 {
              return Err("led color must be #RRGGBB format");
          }
          // Validate hex characters
          for c in self.led_color[1..].chars() {
              if !c.is_ascii_hexdigit() {
                  return Err("led color contains invalid hex characters");
              }
          }
          Ok(())
      }
  }
  ```

---

## Phase 1: GPIO & Display

**Goal**: Get the nixie display showing digits.

### Hardware Pins
| Function | GPIO |
|----------|------|
| Shift Register Data | GPIO16 |
| Shift Register Clock | GPIO17 |
| Shift Register Latch | GPIO18 |
| Separator 1 | GPIO4 |
| Separator 2 | GPIO2 |

### Implementation

```rust
use esp_hal::gpio::{Level, Output, OutputConfig};
use drivers::shift_register::ShiftRegister;
use drivers::nixie_display::NixieDisplay;

// In main():
let mut data_pin = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
let mut clock_pin = Output::new(peripherals.GPIO17, Level::Low, OutputConfig::default());
let mut latch_pin = Output::new(peripherals.GPIO18, Level::Low, OutputConfig::default());
let mut sep1 = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
let mut sep2 = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());

let mut shift_register = ShiftRegister::new(&mut data_pin, &mut clock_pin, &mut latch_pin);
let mut display = NixieDisplay::new(&mut shift_register, &mut sep1, &mut sep2);

// Display loop with 200ms ticker
let mut ticker = Ticker::every(Duration::from_millis(200));
loop {
    ticker.next().await;
    display.display(current_time);
}
```

---

## Phase 2: RGB LED / PWM

**Goal**: Control RGB LED backlight using LEDC PWM.

### Hardware Pins
| Color | GPIO | LEDC Channel |
|-------|------|--------------|
| Red | GPIO27 | Channel0 |
| Green | GPIO26 | Channel1 |
| Blue | GPIO25 | Channel2 |

### Implementation

```rust
use esp_hal::ledc::{Ledc, LowSpeed, LSGlobalClkSource};
use esp_hal::ledc::timer::{self, TimerIFace};
use esp_hal::ledc::channel::{self, ChannelIFace};
use esp_hal::gpio::DriveMode;
use drivers::rgb_led::RgbLed;

let mut ledc = Ledc::new(peripherals.LEDC);
ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

// Configure timer at 5kHz with 8-bit resolution
let mut timer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
timer0.configure(timer::config::Config {
    duty: timer::config::Duty::Duty8Bit,
    clock_source: timer::LSClockSource::APBClk,
    frequency: Rate::from_khz(5),
})?;

// Configure channels
let mut red = ledc.channel(channel::Number::Channel0, peripherals.GPIO27);
red.configure(channel::config::Config {
    timer: &timer0,
    duty_pct: 0,
    drive_mode: DriveMode::PushPull,
})?;
// Similar for green (Channel1, GPIO26) and blue (Channel2, GPIO25)

let mut rgb = RgbLed::new(red, green, blue);
rgb.set_color(0x000088)?; // Blue
```

**Note**: LEDC channels implement `embedded_hal::pwm::SetDutyCycle` which `RgbLed` expects.

---

## Phase 3: Button Input with Debouncing

**Goal**: Read button input with software debouncing.

### Hardware
| Function | GPIO | Config |
|----------|------|--------|
| Button | GPIO19 | Input with Pull-Up (active low) |

### Implementation

```rust
use esp_hal::gpio::{Input, InputConfig, Pull};
use embassy_sync::mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Duration, Ticker};
use drivers::debouncer::Debouncer;

type DebouncerMutex = Mutex<CriticalSectionRawMutex, Debouncer<Input<'static>>>;
static DEBOUNCER: StaticCell<DebouncerMutex> = StaticCell::new();

// In main():
let button_pin = Input::new(peripherals.GPIO19, InputConfig::default().with_pull(Pull::Up));
let debouncer = DEBOUNCER.init(Mutex::new(Debouncer::new(0.1, 100, button_pin)));

spawner.spawn(debounce_task(debouncer)).ok();

#[embassy_executor::task]
async fn debounce_task(debouncer: &'static DebouncerMutex) {
    let mut ticker = Ticker::every(Duration::from_millis(10)); // 100Hz
    loop {
        ticker.next().await;
        debouncer.lock().await.update().ok();
    }
}
```

---

## Phase 4: WiFi Station Mode

**Goal**: Connect to WiFi network with automatic reconnection.

### Implementation

```rust
use esp_radio::wifi::{ModeConfig, WifiController, StationConfig, WifiEvent, WifiStationState};
use embassy_net::{Config, Runner, Stack, StackResources};
use esp_hal::rng::Rng;
use static_cell::StaticCell;

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: StaticCell<$t> = StaticCell::new();
        STATIC_CELL.init($val)
    }};
}

// In main():
let radio_init = esp_radio::init().expect("Failed to init radio");
let (controller, interfaces) = esp_radio::wifi::new(
    &radio_init,
    peripherals.WIFI,
    Default::default(),
).expect("Failed to init WiFi");

// Network stack
let config = Config::dhcpv4(Default::default());
let rng = Rng::new();
let seed = (rng.random() as u64) << 32 | rng.random() as u64;

let (stack, runner) = embassy_net::new(
    interfaces.station,
    config,
    mk_static!(StackResources<3>, StackResources::<3>::new()),
    seed,
);

spawner.spawn(net_task(runner)).ok();
spawner.spawn(wifi_task(controller, ssid, password)).ok();

// Wait for IP
loop {
    if stack.is_link_up() {
        if let Some(config) = stack.config_v4() {
            info!("Got IP: {}", config.address);
            break;
        }
    }
    Timer::after(Duration::from_millis(500)).await;
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

#[embassy_executor::task]
async fn wifi_task(mut controller: WifiController<'static>, ssid: &'static str, password: &'static str) {
    loop {
        match esp_radio::wifi::station_state() {
            WifiStationState::Connected => {
                controller.wait_for_event(WifiEvent::StationDisconnected).await;
                info!("WiFi disconnected, reconnecting...");
                Timer::after(Duration::from_secs(5)).await;
            }
            _ => {}
        }
        
        if !matches!(controller.is_started(), Ok(true)) {
            let config = ModeConfig::Station(
                StationConfig::default()
                    .with_ssid(ssid.try_into().unwrap())
                    .with_password(password.try_into().unwrap()),
            );
            controller.set_config(&config).unwrap();
            controller.start_async().await.unwrap();
        }
        
        match controller.connect_async().await {
            Ok(_) => info!("WiFi connected!"),
            Err(e) => {
                info!("WiFi connect failed: {:?}", e);
                Timer::after(Duration::from_secs(5)).await;
            }
        }
    }
}
```

---

## Phase 5: NTP Time Sync

**Goal**: Synchronize time from NTP server using `sntpc`.

### Dependencies
```toml
sntpc = { version = "0.7", default-features = false, features = ["embassy-socket", "defmt"] }
chrono = { version = "0.4", default-features = false }
chrono-tz = { version = "0.10", default-features = false }
```

### Time Tracking Structure

```rust
use core::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use chrono::{DateTime, Utc};
use embassy_time::Instant;

pub struct TimeKeeper {
    base_timestamp_secs: AtomicI64,  // Unix timestamp from NTP
    base_instant_ms: AtomicU64,       // Instant::now().as_millis() at sync time
    synced: AtomicBool,
}

impl TimeKeeper {
    pub const fn new() -> Self {
        Self {
            base_timestamp_secs: AtomicI64::new(0),
            base_instant_ms: AtomicU64::new(0),
            synced: AtomicBool::new(false),
        }
    }

    pub fn update(&self, unix_timestamp: i64) {
        self.base_timestamp_secs.store(unix_timestamp, Ordering::SeqCst);
        self.base_instant_ms.store(Instant::now().as_millis(), Ordering::SeqCst);
        self.synced.store(true, Ordering::SeqCst);
    }

    pub fn now_utc(&self) -> Option<DateTime<Utc>> {
        if !self.synced.load(Ordering::Relaxed) {
            return None;
        }
        let base_ts = self.base_timestamp_secs.load(Ordering::Relaxed);
        let base_ms = self.base_instant_ms.load(Ordering::Relaxed);
        let elapsed_ms = Instant::now().as_millis() - base_ms;
        let current_ts = base_ts + (elapsed_ms / 1000) as i64;
        DateTime::from_timestamp(current_ts, 0)
    }
}
```

### NTP Timestamp Generator

```rust
use sntpc::{NtpTimestampGenerator, NtpContext};
use embassy_time::Instant;

#[derive(Copy, Clone)]
struct EmbassyTimestamp {
    instant: Instant,
}

impl NtpTimestampGenerator for EmbassyTimestamp {
    fn init(&mut self) {
        self.instant = Instant::now();
    }
    
    fn timestamp_sec(&self) -> u64 {
        self.instant.as_secs()
    }
    
    fn timestamp_subsec_micros(&self) -> u32 {
        (self.instant.as_micros() % 1_000_000) as u32
    }
}

impl Default for EmbassyTimestamp {
    fn default() -> Self {
        Self { instant: Instant::now() }
    }
}
```

### NTP Sync Task

```rust
use embassy_net::udp::UdpSocket;
use sntpc::get_time;

#[embassy_executor::task]
async fn ntp_task(stack: Stack<'static>, time_keeper: &'static TimeKeeper) {
    // Wait for network
    while !stack.is_link_up() {
        Timer::after(Duration::from_millis(500)).await;
    }
    
    let mut rx_buffer = [0; 128];
    let mut tx_buffer = [0; 128];
    let mut rx_meta = [embassy_net::udp::PacketMetadata::EMPTY; 1];
    let mut tx_meta = [embassy_net::udp::PacketMetadata::EMPTY; 1];
    
    loop {
        let mut socket = UdpSocket::new(
            stack,
            &mut rx_meta,
            &mut rx_buffer,
            &mut tx_meta,
            &mut tx_buffer,
        );
        
        // Resolve NTP server (time.google.com = 216.239.35.0)
        let server_addr = (Ipv4Addr::new(216, 239, 35, 0), 123);
        
        let context = NtpContext::new(EmbassyTimestamp::default());
        
        match get_time(server_addr.into(), &socket, context).await {
            Ok(result) => {
                let unix_secs = result.sec() as i64 - 2208988800; // NTP epoch to Unix epoch
                time_keeper.update(unix_secs);
                info!("NTP sync successful: {}", unix_secs);
            }
            Err(e) => {
                info!("NTP sync failed: {:?}", e);
            }
        }
        
        drop(socket);
        Timer::after(Duration::from_secs(3600)).await; // Resync every hour
    }
}
```

---

## Phase 6: HTTP Server (picoserve)

**Goal**: Serve config web UI and REST API.

### Dependencies
```toml
picoserve = { version = "0.17", default-features = false, features = ["embassy-net", "embassy-time", "defmt"] }
serde-json-core = "0.6"
```

### Router Setup

```rust
use picoserve::{
    routing::{get, post},
    response::{IntoResponse, Response, StatusCode},
    Router,
};
use embassy_sync::signal::Signal;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

type ConfigSignal = Signal<CriticalSectionRawMutex, ()>;

struct AppState {
    config: &'static Mutex<CriticalSectionRawMutex, InternalConfig>,
    config_changed: &'static ConfigSignal,
    storage: &'static Mutex<CriticalSectionRawMutex, FlashConfigStorage>,
}

type AppRouter = impl picoserve::routing::PathRouter<AppState>;

fn make_app() -> picoserve::Router<AppRouter, AppState> {
    picoserve::Router::new()
        .route("/", get(index_handler))
        .route("/config", get(get_config_handler).post(post_config_handler))
}
```

### Route Handlers

```rust
// Embed the webapp
const INDEX_HTML: &str = include_str!("../../../webapp/dist/index.html");

async fn index_handler() -> impl IntoResponse {
    Response::ok(INDEX_HTML).with_header("Content-Type", "text/html")
}

async fn get_config_handler(
    picoserve::extract::State(state): picoserve::extract::State<AppState>,
) -> impl IntoResponse {
    let config = state.config.lock().await;
    let api_config: Config = (*config).clone().into();
    
    let mut buf = [0u8; 256];
    match serde_json_core::to_slice(&api_config, &mut buf) {
        Ok(len) => Response::ok(&buf[..len]).with_header("Content-Type", "application/json"),
        Err(_) => Response::new(StatusCode::INTERNAL_SERVER_ERROR, "Serialization error"),
    }
}

async fn post_config_handler(
    picoserve::extract::State(state): picoserve::extract::State<AppState>,
    body: &[u8],
) -> impl IntoResponse {
    // Parse JSON
    let api_config: Config = match serde_json_core::from_slice(body) {
        Ok((config, _)) => config,
        Err(_) => return Response::new(StatusCode::BAD_REQUEST, "Invalid JSON"),
    };
    
    // Validate
    if let Err(msg) = api_config.validate() {
        return Response::new(StatusCode::BAD_REQUEST, msg);
    }
    
    // Convert and save
    let internal: InternalConfig = api_config.into();
    
    {
        let mut storage = state.storage.lock().await;
        if let Err(_) = storage.save(&internal) {
            return Response::new(StatusCode::INTERNAL_SERVER_ERROR, "Storage error");
        }
    }
    
    {
        let mut config = state.config.lock().await;
        *config = internal;
    }
    
    // Signal main task
    state.config_changed.signal(());
    
    Response::ok("OK")
}
```

### HTTP Server Task

```rust
#[embassy_executor::task]
async fn http_task(
    stack: Stack<'static>,
    app: &'static picoserve::Router<AppRouter, AppState>,
    state: AppState,
) {
    let config = picoserve::Config::new(picoserve::Timeouts {
        start_read_request: Some(Duration::from_secs(5)),
        read_request: Some(Duration::from_secs(1)),
        write: Some(Duration::from_secs(1)),
    })
    .keep_connection_alive();

    loop {
        let mut rx_buffer = [0; 1536];
        let mut tx_buffer = [0; 1536];
        
        let mut socket = embassy_net::tcp::TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        
        if socket.accept(80).await.is_err() {
            continue;
        }
        
        let _ = picoserve::serve(app, &config, &mut socket, &state).await;
    }
}
```

---

## Phase 7: Flash Storage

**Goal**: Persist configuration across reboots.

### Dependencies
```toml
esp-storage = { version = "0.4", features = ["esp32"] }
```

### Storage Format

```
Offset   Content
0x00     Magic: 0x4E495849 ("NIXI")
0x04     Version: u32 (currently 1)
0x08     Length: u16 (postcard data length)
0x0A     Data: postcard-serialized InternalConfig
```

### Flash Storage Implementation

```rust
use esp_storage::FlashStorage;
use embedded_storage::{ReadStorage, Storage as EmbeddedStorage};
use drivers::storage::{Storage, StorageError};
use postcard::{from_bytes, to_vec};

const CONFIG_FLASH_OFFSET: u32 = 0x3F0000; // Adjust based on partition table
const MAGIC: u32 = 0x4E495849; // "NIXI"
const VERSION: u32 = 1;

pub struct FlashConfigStorage {
    flash: FlashStorage,
}

impl FlashConfigStorage {
    pub fn new(flash: FlashStorage) -> Self {
        Self { flash }
    }

    pub fn load(&mut self) -> Result<Option<InternalConfig>, StorageError> {
        let mut header = [0u8; 10];
        self.flash.read(CONFIG_FLASH_OFFSET, &mut header)
            .map_err(|_| StorageError::ReadError)?;
        
        // Check magic
        let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        if magic != MAGIC {
            return Ok(None);
        }
        
        // Check version
        let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if version != VERSION {
            return Ok(None);
        }
        
        // Read length
        let len = u16::from_le_bytes([header[8], header[9]]) as usize;
        if len > 256 {
            return Ok(None);
        }
        
        // Read data
        let mut data = [0u8; 256];
        self.flash.read(CONFIG_FLASH_OFFSET + 10, &mut data[..len])
            .map_err(|_| StorageError::ReadError)?;
        
        match from_bytes::<InternalConfig>(&data[..len]) {
            Ok(config) => Ok(Some(config)),
            Err(_) => Ok(None),
        }
    }

    pub fn save(&mut self, config: &InternalConfig) -> Result<(), StorageError> {
        let data = to_vec::<InternalConfig, 256>(config)
            .map_err(|_| StorageError::WriteError)?;
        
        let mut buffer = [0u8; 266]; // 10 header + 256 data max
        buffer[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        buffer[4..8].copy_from_slice(&VERSION.to_le_bytes());
        buffer[8..10].copy_from_slice(&(data.len() as u16).to_le_bytes());
        buffer[10..10 + data.len()].copy_from_slice(&data);
        
        self.flash.write(CONFIG_FLASH_OFFSET, &buffer[..10 + data.len()])
            .map_err(|_| StorageError::WriteError)?;
        
        Ok(())
    }
}
```

---

## New File Structure

```
clock-no_std/
├── src/
│   ├── bin/
│   │   └── main.rs          # Entry point, task spawning, main loop
│   ├── lib.rs               # Crate root, module declarations
│   ├── wifi.rs              # WiFi connection task
│   ├── http.rs              # HTTP server + picoserve routes
│   ├── ntp.rs               # NTP sync task + TimeKeeper
│   └── storage.rs           # FlashConfigStorage implementation
├── Cargo.toml
└── ...
```

---

## Dependencies to Add to `clock-no_std/Cargo.toml`

```toml
# Local crate
drivers = { path = "../drivers", default-features = false }

# HTTP server
picoserve = { version = "0.17", default-features = false, features = ["embassy-net", "embassy-time", "defmt"] }
serde-json-core = "0.6"

# NTP
sntpc = { version = "0.7", default-features = false, features = ["embassy-socket", "defmt"] }

# Time
chrono = { version = "0.4", default-features = false }
chrono-tz = { version = "0.10", default-features = false }

# Storage
esp-storage = { version = "0.4", features = ["esp32"] }

# Serialization (for config)
serde = { version = "1.0", default-features = false, features = ["derive", "alloc"] }
postcard = { version = "1.1", default-features = false, features = ["alloc"] }

# Async sync primitives
embassy-sync = "0.6"

# Utilities
heapless = "0.8"  # For fixed-size collections if needed
```

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                            Main Task                                 │
│  - Initialize peripherals (GPIO, LEDC, WiFi, Flash)                 │
│  - Load config from flash                                           │
│  - Spawn all async tasks                                            │
│  - Main loop: update display every 200ms, handle button press       │
└─────────────────────────────────────────────────────────────────────┘
                                   │
         ┌─────────────────────────┼─────────────────────────┐
         │                         │                         │
         ▼                         ▼                         ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   WiFi Task     │    │  Net Runner     │    │ Debounce Task   │
│                 │    │     Task        │    │                 │
│ - Connect       │    │                 │    │ - 100Hz ticker  │
│ - Reconnect     │    │ - embassy-net   │    │ - Update state  │
│ - Monitor       │    │   packet proc   │    │                 │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                         │
         │          ┌──────────────┴──────────────┐
         │          │                             │
         │          ▼                             ▼
         │ ┌─────────────────┐         ┌─────────────────┐
         │ │   NTP Task      │         │  HTTP Task      │
         │ │                 │         │                 │
         │ │ - Sync hourly   │         │ - picoserve     │
         │ │ - Update time   │         │ - Serve UI      │
         │ │   keeper        │         │ - REST API      │
         │ └─────────────────┘         └─────────────────┘
         │          │                             │
         ▼          ▼                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        Shared State (static)                         │
│                                                                      │
│  TimeKeeper          Config Mutex           Config Signal            │
│  ├─ base_timestamp   ├─ InternalConfig      └─ Signal<()>           │
│  ├─ base_instant     └─ (wifi, tz, led,                              │
│  └─ synced               hours_24)                                   │
│                                                                      │
│  Debouncer Mutex     Flash Storage Mutex                             │
│  └─ Debouncer        └─ FlashConfigStorage                          │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Task Checklist

| Phase | Task | Priority | Status |
|-------|------|----------|--------|
| 0 | Modify `drivers/Cargo.toml` for no_std | High | ⬜ |
| 0 | Add `#![no_std]` and alloc to `drivers/src/lib.rs` | High | ⬜ |
| 0 | Update `drivers/src/storage.rs` (hashbrown, core::cmp) | High | ⬜ |
| 0 | Update `drivers/src/config.rs` (alloc, manual validation) | High | ⬜ |
| 0 | Test drivers crate compiles with `--no-default-features` | High | ⬜ |
| 1 | Configure GPIO outputs for shift register | High | ⬜ |
| 1 | Configure GPIO outputs for separators | High | ⬜ |
| 1 | Integrate ShiftRegister + NixieDisplay | High | ⬜ |
| 1 | Add display update loop with Ticker | High | ⬜ |
| 2 | Configure LEDC peripheral | Medium | ⬜ |
| 2 | Integrate RgbLed driver | Medium | ⬜ |
| 3 | Configure GPIO19 input with pull-up | Medium | ⬜ |
| 3 | Integrate Debouncer with async task | Medium | ⬜ |
| 4 | Initialize WiFi with esp-radio | High | ⬜ |
| 4 | Create embassy-net stack | High | ⬜ |
| 4 | Spawn net_task and wifi_task | High | ⬜ |
| 5 | Implement TimeKeeper | High | ⬜ |
| 5 | Implement NtpTimestampGenerator | High | ⬜ |
| 5 | Spawn ntp_task | High | ⬜ |
| 5 | Integrate chrono-tz for timezone conversion | High | ⬜ |
| 6 | Define picoserve routes | High | ⬜ |
| 6 | Implement GET / (serve HTML) | High | ⬜ |
| 6 | Implement GET /config | High | ⬜ |
| 6 | Implement POST /config with validation | High | ⬜ |
| 6 | Spawn http_task | High | ⬜ |
| 7 | Implement FlashConfigStorage | Medium | ⬜ |
| 7 | Load config on boot | Medium | ⬜ |
| 7 | Save config on HTTP POST | Medium | ⬜ |

---

## Notes

- **Memory**: 98KB heap available via `esp_alloc`
- **WiFi credentials**: Initially hardcoded or loaded from flash; no AP fallback in first version
- **Error handling**: Use `defmt::error!()` for logging, return `Result` where appropriate
- **Testing**: Hardware testing with Wokwi simulator (diagram.json already exists)
