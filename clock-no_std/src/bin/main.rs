#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use clock_no_std::simple_nixie::SimpleNixie;
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
use chrono::DateTime;
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

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Global signal for button press events
static BUTTON_PRESS_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();


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

    // Spawn display update task that handles mode cycling
    if spawner.spawn(display_update_task(data_pin, clock_pin, latch_pin)).is_err() {
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
    latch_pin: &'static mut Output<'static>
) {
    let mut nixie_display = SimpleNixie::new(data_pin, clock_pin, latch_pin);
    
    // Initial test display
    if let Err(_e) = nixie_display.test_pattern() {
        println!("Test pattern failed");
    }
    
    let mut counter = 0u64;
    
    loop {
        
        // Check for button press signal
        if BUTTON_PRESS_SIGNAL.try_take().is_some() {
            println!("*** MODE CHANGE RECEIVED IN DISPLAY TASK! ***");
            nixie_display.next_mode();
            println!("Mode cycled, new mode: {:?}", nixie_display.get_mode());
        }
        
        // Update display every 2 seconds with incrementing fake time
        if counter % 40 == 0 { // Every 2 seconds (40 * 50ms)
            // Use fake time that increments
            let fake_timestamp = 1640995200 + (counter / 40) * 60; // Add 1 minute each update
            if let Some(fake_time) = DateTime::from_timestamp(fake_timestamp as i64, 0) {
                if let Err(_e) = nixie_display.update(&fake_time) {
                    println!("Display update failed");
                }
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
