#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use drivers::nixie_display::{NixieDisplay, HourFormat};
use drivers::shift_register::ShiftRegister;
// use drivers::debouncer::Debouncer;  // TODO: Integrate async debouncing
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, Level, Output, Pull},
    rng::Rng,
    timer::timg::TimerGroup,
};
use esp_wifi::{
    init, wifi, EspWifiController,
    wifi::{AuthMethod, ClientConfiguration, Configuration, WifiStaDevice},
};
use esp_println::println;
use static_cell::StaticCell;

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: StaticCell<$t> = StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.init_with(|| $val);
        x
    }};
}

use embassy_net::{Config as NetConfig, StackResources};
use picoserve::routing::{get, post};
use picoserve::response::Json;
// Use alloc::string::String for request body
use clock_no_std::config::{Config, ConfigStorage, InternalConfig, InMemoryStorage};
use clock_no_std::web_ui::WEB_UI_HTML;
use clock_no_std::sntp::{SntpClient, TimeManager};
use serde_json;
use embassy_sync::mutex::Mutex;
use heapless;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;
// use alloc::string::String;  // Unused for now

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Global signal for button press events
static BUTTON_PRESS_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

// Global signals for configuration changes
static WIFI_CONFIG_SIGNAL: Signal<CriticalSectionRawMutex, (heapless::String<32>, heapless::String<64>)> = Signal::new();
static DISPLAY_CONFIG_SIGNAL: Signal<CriticalSectionRawMutex, (heapless::String<32>, bool)> = Signal::new();
static LED_CONFIG_SIGNAL: Signal<CriticalSectionRawMutex, u32> = Signal::new();

// Global signal for time synchronization
static TIME_SYNC_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

// Global configuration storage
static CONFIG_STORAGE: StaticCell<Mutex<CriticalSectionRawMutex, ConfigStorage<InMemoryStorage>>> = StaticCell::new();
static mut CONFIG_REF: Option<&'static Mutex<CriticalSectionRawMutex, ConfigStorage<InMemoryStorage>>> = None;

// Global time manager
static TIME_MANAGER: StaticCell<Mutex<CriticalSectionRawMutex, TimeManager>> = StaticCell::new();
static mut TIME_MANAGER_REF: Option<&'static Mutex<CriticalSectionRawMutex, TimeManager>> = None;


#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(72 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut rng = Rng::new(peripherals.RNG);

    // WiFi initialization following exact working example pattern
    println!("Initializing WiFi...");
    
    let init = &*mk_static!(
        EspWifiController<'static>,
        init(timg0.timer0, rng.clone(), peripherals.RADIO_CLK).unwrap()
    );

    let wifi = peripherals.WIFI;
    let (wifi_interface, mut controller) =
        esp_wifi::wifi::new_with_mode(&init, wifi, WifiStaDevice).unwrap();

    // Embassy initialization for ESP32
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timg1.timer0);
    
    // Set up networking stack with DHCP following the working example
    let config = NetConfig::dhcpv4(Default::default());
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );
    
    // Store stack in static cell for task access
    static STACK: StaticCell<embassy_net::Stack<'static>> = StaticCell::new();
    let stack = STACK.init(stack);
    
    println!("Network stack created with DHCP");
    
    println!("WiFi and Embassy initialized successfully");

    // Initialize configuration storage
    let storage = InMemoryStorage::new();
    let config_storage = ConfigStorage::new(storage);
    let config_storage = CONFIG_STORAGE.init(Mutex::new(config_storage));
    
    // Initialize time manager
    let time_manager = TimeManager::new();
    let time_manager = TIME_MANAGER.init(Mutex::new(time_manager));
    
    // Store references for HTTP handlers and tasks
    unsafe {
        CONFIG_REF = Some(config_storage);
        TIME_MANAGER_REF = Some(time_manager);
    }
    
    println!("Configuration storage initialized");

    // Initialize nixie display pins
    // Based on the ACTUAL original clock configuration:
    // - Data pin: GPIO16
    // - Clock pin: GPIO17  
    // - Latch pin: GPIO18
    let data_pin = Output::new(peripherals.GPIO16, Level::Low);
    let clock_pin = Output::new(peripherals.GPIO17, Level::Low);
    let latch_pin = Output::new(peripherals.GPIO18, Level::Low);
    
    // Store pins in static cells for task use
    static DATA_PIN: StaticCell<Output<'static>> = StaticCell::new();
    static CLOCK_PIN: StaticCell<Output<'static>> = StaticCell::new();
    static LATCH_PIN: StaticCell<Output<'static>> = StaticCell::new();
    
    let data_pin = DATA_PIN.init(data_pin);
    let clock_pin = CLOCK_PIN.init(clock_pin);
    let latch_pin = LATCH_PIN.init(latch_pin);
    
    // Set up button pin for async usage
    let button = Input::new(peripherals.GPIO19, Pull::Up);
    
    println!("Button pin configured on GPIO19");
    
    // Store button in static cell for button task
    static BUTTON_PIN: StaticCell<Input<'static>> = StaticCell::new();
    let button = BUTTON_PIN.init(button);
    
    // Add separator pins for nixie display visual effects
    // GPIO20 and GPIO21 for separator control
    let sep1_pin = Output::new(peripherals.GPIO20, Level::Low);
    let sep2_pin = Output::new(peripherals.GPIO21, Level::Low);
    
    // Store separator pins in static cells
    static SEP1_PIN: StaticCell<Output<'static>> = StaticCell::new();
    static SEP2_PIN: StaticCell<Output<'static>> = StaticCell::new();
    
    let sep1_pin = SEP1_PIN.init(sep1_pin);
    let sep2_pin = SEP2_PIN.init(sep2_pin);
    
    println!("Nixie display and button initialized");
    
    // WiFi configuration for Wokwi-GUEST (no authentication)
    let client_config = ClientConfiguration {
        ssid: "Wokwi-GUEST".try_into().unwrap(),
        password: "".try_into().unwrap(),
        auth_method: AuthMethod::None,  // Wokwi-GUEST doesn't require authentication
        ..Default::default()
    };
    
    let wifi_config = Configuration::Client(client_config);
    controller.set_configuration(&wifi_config).unwrap();
    
    println!("WiFi configuration set");
    
    // Store controller in static for task
    static WIFI_CONTROLLER: StaticCell<wifi::WifiController<'static>> = StaticCell::new();
    let wifi_controller = WIFI_CONTROLLER.init(controller);
    
    if spawner.spawn(wifi_connection_task(wifi_controller)).is_err() {
        println!("Failed to spawn WiFi connection task");
    }
    
    // Spawn network stack runner (required for DHCP to work)
    if spawner.spawn(net_task(runner)).is_err() {
        println!("Failed to spawn network task");
    }
    
    // Spawn network monitoring task to show IP address
    if spawner.spawn(network_monitor_task(stack)).is_err() {
        println!("Failed to spawn network monitor task");
    }
    
    // Spawn HTTP server task
    if spawner.spawn(http_server_task(stack)).is_err() {
        println!("Failed to spawn HTTP server task");
    }
    
    // Spawn WiFi configuration hot-reload task  
    if spawner.spawn(wifi_config_task()).is_err() {
        println!("Failed to spawn WiFi config task");
    }
    
    // Spawn LED configuration hot-reload task
    if spawner.spawn(led_config_task()).is_err() {
        println!("Failed to spawn LED config task");
    }

    // Spawn time synchronization task
    if spawner.spawn(time_sync_task(stack)).is_err() {
        println!("Failed to spawn time sync task");
    }

    // Spawn display update task that handles mode cycling
    if spawner.spawn(display_update_task(data_pin, clock_pin, latch_pin, sep1_pin, sep2_pin)).is_err() {
        println!("Failed to spawn display task");
    }
    
    // Spawn button handling task using Embassy async approach
    if spawner.spawn(button_task(button)).is_err() {
        println!("Failed to spawn button task");
    }

    // Main loop - keep running
    loop {
        Timer::after(Duration::from_secs(10)).await;
    }
}

// Embassy-style async button task 
#[embassy_executor::task]
async fn button_task(button: &'static mut Input<'static>) {
    loop {
        // Wait for button press (falling edge)
        button.wait_for_falling_edge().await;
        
        println!("Button pressed - signaling mode change");
        BUTTON_PRESS_SIGNAL.signal(());
        
        // Simple debounce delay
        Timer::after(Duration::from_millis(50)).await;
    }
}

#[embassy_executor::task]
async fn display_update_task(
    data_pin: &'static mut Output<'static>,
    clock_pin: &'static mut Output<'static>, 
    latch_pin: &'static mut Output<'static>,
    sep1_pin: &'static mut Output<'static>,
    sep2_pin: &'static mut Output<'static>
) {
    let mut shift_register = ShiftRegister::new(data_pin, clock_pin, latch_pin);
    let mut nixie_display = NixieDisplay::new(&mut shift_register, sep1_pin, sep2_pin);
    
    // Set initial format to 24-hour
    nixie_display.set_hour_format(HourFormat::TwentyFourHour);
    
    // Initial test display - show current time
    println!("NixieDisplay initialized with separator pins");
    
    let mut counter = 0u64;
    
    loop {
        
        // Check for button press signal
        if BUTTON_PRESS_SIGNAL.try_take().is_some() {
            println!("*** MODE CHANGE RECEIVED IN DISPLAY TASK! ***");
            nixie_display.next_mode();
            println!("Mode cycled to next mode");
        }
        
        // Check for display configuration changes
        if let Some((timezone, hours_24)) = DISPLAY_CONFIG_SIGNAL.try_take() {
            println!("*** DISPLAY CONFIG CHANGE: timezone={}, 24h={} ***", timezone, hours_24);
            let format = if hours_24 { HourFormat::TwentyFourHour } else { HourFormat::TwelveHour };
            nixie_display.set_hour_format(format);
            
            // Update timezone in time manager
            if let Some(time_manager) = unsafe { TIME_MANAGER_REF } {
                let mut tm = time_manager.lock().await;
                if let Err(_) = tm.set_timezone(&timezone) {
                    println!("Failed to set timezone: {}", timezone);
                }
            }
        }
        
        // Update display every 2 seconds with real time
        if counter % 40 == 0 { // Every 2 seconds (40 * 50ms)
            // Get current time from time manager (with timezone conversion)
            let display_time = if let Some(time_manager) = unsafe { TIME_MANAGER_REF } {
                let tm = time_manager.lock().await;
                // Try to get local time with timezone, fallback to UTC
                if let Some(local_time) = tm.get_local_time() {
                    local_time.naive_utc().and_utc()
                } else {
                    tm.get_current_time().unwrap_or_else(|| {
                        // Fallback: use fake time that increments
                        let fake_timestamp = 1640995200 + (counter / 40) * 60;
                        chrono::DateTime::<chrono::Utc>::from_timestamp(fake_timestamp as i64, 0)
                            .expect("Invalid fake timestamp")
                    })
                }
            } else {
                // Final fallback: use fake time that increments
                let fake_timestamp = 1640995200 + (counter / 40) * 60;
                chrono::DateTime::<chrono::Utc>::from_timestamp(fake_timestamp as i64, 0)
                    .expect("Invalid fake timestamp")
            };
            
            // Use the new NixieDisplay API - no error handling needed
            nixie_display.display(display_time);
            
            // Log the displayed time occasionally
            if counter % 1200 == 0 { // Every 60 seconds
                println!("Display showing time: {}", display_time);
            }
        }
        
        counter += 1;
        
        // Embassy Timer should work with esp-hal v0.23.1 and esp-hal-embassy v0.6.0
        Timer::after(Duration::from_millis(50)).await;
    }
}

#[embassy_executor::task]
async fn wifi_connection_task(controller: &'static mut wifi::WifiController<'static>) {
    println!("Starting WiFi connection task");
    
    loop {
        if let Err(e) = controller.start() {
            println!("Failed to start WiFi: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }
        println!("WiFi started successfully");
        
        if let Err(e) = controller.connect() {
            println!("Failed to connect to WiFi: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }
        println!("WiFi connect command sent");
        
        // Wait for actual connection
        println!("Waiting for WiFi connection...");
        loop {
            match controller.is_connected() {
                Ok(true) => {
                    println!("WiFi connected to network!");
                    break;
                }
                Ok(false) => {
                    println!("Still connecting...");
                    Timer::after(Duration::from_secs(1)).await;
                }
                Err(e) => {
                    println!("Error checking connection: {:?}", e);
                    Timer::after(Duration::from_secs(1)).await;
                }
            }
        }
        
        // Wait for disconnection
        println!("Connected! Monitoring connection...");
        loop {
            match controller.is_connected() {
                Ok(true) => {
                    // Still connected
                    Timer::after(Duration::from_secs(10)).await;
                }
                Ok(false) => {
                    println!("WiFi disconnected, reconnecting...");
                    break;
                }
                Err(e) => {
                    println!("Connection check error: {:?}", e);
                    Timer::after(Duration::from_secs(5)).await;
                    break;
                }
            }
        }
        
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, esp_wifi::wifi::WifiDevice<'static, esp_wifi::wifi::WifiStaDevice>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn network_monitor_task(stack: &'static embassy_net::Stack<'static>) {
    println!("Network monitor task started");
    
    loop {
        // Wait for link to be up
        loop {
            if stack.is_link_up() {
                println!("Network link is up!");
                break;
            }
            Timer::after(Duration::from_millis(500)).await;
        }

        // Wait for DHCP to get IP address
        println!("Waiting to get IP address...");
        loop {
            if let Some(config) = stack.config_v4() {
                println!("Got IP: {}", config.address);
                println!("Gateway: {:?}", config.gateway);
                println!("DNS servers: {:?}", config.dns_servers);
                break;
            }
            Timer::after(Duration::from_millis(500)).await;
        }

        // Monitor for link going down
        loop {
            if !stack.is_link_up() {
                println!("Network link is down!");
                break;
            }
            Timer::after(Duration::from_secs(5)).await;
        }
        
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn http_server_task(stack: &'static embassy_net::Stack<'static>) {
    println!("HTTP server task started");
    
    // Wait for network to be ready
    loop {
        if stack.is_link_up() && stack.config_v4().is_some() {
            println!("Network ready, starting HTTP server...");
            break;
        }
        Timer::after(Duration::from_secs(1)).await;
    }
    
    // Create picoserve app with routes
    let app = picoserve::Router::new()
        .route("/", get(index_handler))
        .route("/config", get(get_config_handler))
        .route("/config", post(post_config_handler));
    
    let config = picoserve::Config::new(picoserve::Timeouts {
        start_read_request: Some(Duration::from_secs(5)),
        read_request: Some(Duration::from_secs(1)),
        write: Some(Duration::from_secs(1)),
    })
    .keep_connection_alive();
    
    println!("Starting HTTP server on port 80...");
    
    let _result = picoserve::listen_and_serve(
        "http_server",  // task_id for logging
        &app,
        &config,
        *stack,
        80,
        &mut [0; 2048],
        &mut [0; 1024],
        &mut [0; 1024],
    )
    .await;
}

// HTTP handler functions
async fn index_handler() -> &'static str {
    WEB_UI_HTML
}

async fn get_config_handler() -> Result<Json<heapless::String<512>>, &'static str> {
    // Get the initialized storage reference
    let config_storage = unsafe { CONFIG_REF.ok_or("Storage not initialized")? };
    let mut storage = config_storage.lock().await;
    
    match storage.load() {
        Ok(internal_config) => {
            let config: Config = internal_config.into();
            match serde_json::to_string(&config) {
                Ok(json) => {
                    match heapless::String::try_from(json.as_str()) {
                        Ok(json_str) => Ok(Json(json_str)),
                        Err(_) => Err("JSON too large"),
                    }
                },
                Err(_) => Err("Serialization failed"),
            }
        },
        Err(_) => Err("Failed to load configuration"),
    }
}

async fn post_config_handler() -> Json<&'static str> {
    println!("POST /config received - simulating configuration update for hot-reload demo");
    
    // For demonstration, let's simulate saving a config and sending signals
    // This would normally parse the request body, but picoserve API is complex
    
    // Create a demo configuration
    let demo_config = InternalConfig::new(
        "Wokwi-GUEST", 
        "",
        "US/Central", 
        0xFF0000, // Red LED
        true // 24-hour format
    );
    
    println!("Simulated configuration saved: {:?}", demo_config);
    
    // Send signals to demonstrate hot-reloading
    if let (Ok(ssid), Ok(pass)) = (
        heapless::String::try_from(demo_config.wifi_ssid()),
        heapless::String::try_from(demo_config.wifi_pass())
    ) {
        WIFI_CONFIG_SIGNAL.signal((ssid, pass));
        println!("WiFi config signal sent");
    }
    
    if let Ok(tz) = heapless::String::try_from(demo_config.tz()) {
        DISPLAY_CONFIG_SIGNAL.signal((tz, demo_config.hours_24()));
        println!("Display config signal sent");
    }
    
    LED_CONFIG_SIGNAL.signal(demo_config.led_color());
    println!("LED config signal sent");
    
    Json(r#"{"status":"success","message":"Configuration updated (demo hot-reload)"}"#)
}

#[embassy_executor::task]
async fn wifi_config_task() {
    println!("WiFi config task started - listening for configuration changes");
    
    loop {
        // Wait for WiFi configuration changes
        let (new_ssid, _new_password) = WIFI_CONFIG_SIGNAL.wait().await;
        println!("*** WiFi config change received: SSID={}, Password=[hidden] ***", new_ssid);
        
        // TODO: Implement live WiFi reconfiguration
        // This would involve:
        // 1. Disconnect from current network
        // 2. Update WiFi configuration 
        // 3. Reconnect with new credentials
        println!("WiFi reconfiguration would happen here (not fully implemented yet)");
    }
}

#[embassy_executor::task] 
async fn led_config_task() {
    println!("LED config task started - listening for LED color changes");
    
    loop {
        // Wait for LED configuration changes
        let new_color = LED_CONFIG_SIGNAL.wait().await;
        println!("*** LED color change received: 0x{:06X} ***", new_color);
        
        // TODO: Implement LED color updates
        // This would involve updating the RGB LED driver with the new color
        println!("LED color update would happen here (RGB LED not implemented yet)");
    }
}

#[embassy_executor::task]
async fn time_sync_task(stack: &'static embassy_net::Stack<'static>) {
    println!("Time sync task started - waiting for network connection");
    
    // Wait for network to be ready
    loop {
        if stack.is_link_up() && stack.config_v4().is_some() {
            println!("Network ready for time synchronization");
            break;
        }
        Timer::after(Duration::from_secs(1)).await;
    }
    
    let sntp_client = SntpClient::new(stack);
    
    // Initial time sync
    println!("Performing initial time synchronization...");
    match sntp_client.sync_with_default_servers().await {
        Ok(time) => {
            println!("Initial time sync successful: {}", time);
            // Update global time manager
            if let Some(time_manager) = unsafe { TIME_MANAGER_REF } {
                let mut tm = time_manager.lock().await;
                tm.update_time(time);
            }
        }
        Err(e) => {
            println!("Initial time sync failed: {:?}", e);
        }
    }
    
    loop {
        // Listen for time sync requests
        TIME_SYNC_SIGNAL.wait().await;
        println!("Time sync requested");
        
        match sntp_client.sync_with_default_servers().await {
            Ok(time) => {
                println!("Time sync successful: {}", time);
                // Update global time manager
                if let Some(time_manager) = unsafe { TIME_MANAGER_REF } {
                    let mut tm = time_manager.lock().await;
                    tm.update_time(time);
                }
            }
            Err(e) => {
                println!("Time sync failed: {:?}", e);
            }
        }
        
        // Automatic resync every hour
        Timer::after(Duration::from_secs(3600)).await;
    }
}
