#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker};
#[cfg(feature = "wifi")]
use embassy_time::Timer;
use embedded_hal::digital::InputPin;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{DriveMode, Input, InputConfig, Level, Output, OutputConfig, Pull},
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{self, ChannelIFace},
        timer::{self, TimerIFace},
    },
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_println::println;
use static_cell::StaticCell;

#[cfg(feature = "wifi")]
use embassy_net::{Runner, StackResources};
#[cfg(feature = "wifi")]
use esp_hal::rng::Rng;
#[cfg(feature = "wifi")]
use esp_radio::wifi::{
    ClientConfig, ModeConfig, ScanConfig, WifiController, WifiDevice, WifiEvent, WifiStaState,
};

use chrono::NaiveDateTime;
use drivers::debouncer::Debouncer;
use drivers::nixie_display::{HourFormat, NixieDisplay};
use drivers::rgb_led::RgbLed;
use drivers::shift_register::ShiftRegister;

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

// WiFi credentials - set via environment variables at build time
// Build with: WIFI_SSID="YourNetwork" WIFI_PASSWORD="YourPassword" cargo build --release --features wifi
#[cfg(feature = "wifi")]
const WIFI_SSID: &str = env!("WIFI_SSID");
#[cfg(feature = "wifi")]
const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

// Type aliases for cleaner code
type ButtonDebouncer = Debouncer<Input<'static>>;
type DebouncerMutex = Mutex<CriticalSectionRawMutex, ButtonDebouncer>;

// Static storage for shared state
static DEBOUNCER: StaticCell<DebouncerMutex> = StaticCell::new();

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
#[cfg(feature = "wifi")]
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: StaticCell<$t> = StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

/// Task to poll the button debouncer at 100Hz
#[embassy_executor::task]
async fn debounce_task(debouncer: &'static DebouncerMutex) {
    let mut ticker = Ticker::every(Duration::from_millis(10));
    loop {
        ticker.next().await;
        let mut guard = debouncer.lock().await;
        guard.update().ok();
    }
}

/// Task to run the embassy-net network stack
#[cfg(feature = "wifi")]
#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

/// Task to manage WiFi connection and reconnection
#[cfg(feature = "wifi")]
#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.capabilities());

    loop {
        if esp_radio::wifi::sta_state() == WifiStaState::Connected {
            // Wait for disconnection
            controller.wait_for_event(WifiEvent::StaDisconnected).await;
            Timer::after(Duration::from_millis(5000)).await;
        }

        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = ModeConfig::Client(
                ClientConfig::default()
                    .with_ssid(WIFI_SSID.into())
                    .with_password(WIFI_PASSWORD.into()),
            );
            controller.set_config(&client_config).unwrap();
            println!("Starting wifi");
            controller.start_async().await.unwrap();
            println!("Wifi started!");

            println!("Scan");
            let scan_config = ScanConfig::default().with_max(10);
            let result = controller
                .scan_with_config_async(scan_config)
                .await
                .unwrap();
            for ap in result {
                println!("{:?}", ap);
            }
        }

        println!("About to connect...");
        match controller.connect_async().await {
            Ok(_) => println!("Wifi connected!"),
            Err(e) => {
                println!("Failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    println!("Embassy initialized!");

    // =========================================================================
    // WiFi Setup (only when wifi feature is enabled)
    // =========================================================================

    #[cfg(feature = "wifi")]
    let stack = {
        println!("Initializing WiFi...");
        let esp_radio_ctrl =
            &*mk_static!(esp_radio::Controller<'static>, esp_radio::init().unwrap());
        let (controller, interfaces) =
            esp_radio::wifi::new(esp_radio_ctrl, peripherals.WIFI, Default::default()).unwrap();

        let wifi_interface = interfaces.sta;

        println!("WiFi controller initialized");

        // Create network stack
        let net_config = embassy_net::Config::dhcpv4(Default::default());
        let rng = Rng::new();
        let seed = (rng.random() as u64) << 32 | rng.random() as u64;

        let (stack, runner) = embassy_net::new(
            wifi_interface,
            net_config,
            mk_static!(StackResources<3>, StackResources::<3>::new()),
            seed,
        );

        // Spawn network tasks
        spawner.spawn(connection(controller)).ok();
        spawner.spawn(net_task(runner)).ok();

        println!("Network tasks spawned");
        stack
    };

    #[cfg(not(feature = "wifi"))]
    println!("WiFi disabled (enable with --features wifi)");

    // =========================================================================
    // GPIO Setup
    // =========================================================================

    // Shift register pins
    let mut data_pin = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    let mut clock_pin = Output::new(peripherals.GPIO17, Level::Low, OutputConfig::default());
    let mut latch_pin = Output::new(peripherals.GPIO18, Level::Low, OutputConfig::default());

    // Separator LEDs
    let sep1 = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
    let sep2 = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());

    // Button input with pull-up (active low)
    let button_config = InputConfig::default().with_pull(Pull::Up);
    let button_pin = Input::new(peripherals.GPIO19, button_config);

    println!("GPIO configured");

    // =========================================================================
    // LEDC PWM Setup for RGB LED
    // =========================================================================

    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    // Configure timer at 5kHz with 8-bit resolution
    let mut lstimer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    lstimer0
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty8Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(5),
        })
        .expect("Failed to configure LEDC timer");

    // Configure RGB channels (active-low for common anode LED)
    // Red on GPIO27, Channel0
    let mut red_channel = ledc.channel(channel::Number::Channel0, peripherals.GPIO27);
    red_channel
        .configure(channel::config::Config {
            timer: &lstimer0,
            duty_pct: 100, // Start off (inverted)
            drive_mode: DriveMode::PushPull,
        })
        .expect("Failed to configure red channel");

    // Green on GPIO26, Channel1
    let mut green_channel = ledc.channel(channel::Number::Channel1, peripherals.GPIO26);
    green_channel
        .configure(channel::config::Config {
            timer: &lstimer0,
            duty_pct: 100, // Start off (inverted)
            drive_mode: DriveMode::PushPull,
        })
        .expect("Failed to configure green channel");

    // Blue on GPIO25, Channel2
    let mut blue_channel = ledc.channel(channel::Number::Channel2, peripherals.GPIO25);
    blue_channel
        .configure(channel::config::Config {
            timer: &lstimer0,
            duty_pct: 100, // Start off (inverted)
            drive_mode: DriveMode::PushPull,
        })
        .expect("Failed to configure blue channel");

    // Create RGB LED driver
    let mut rgb = RgbLed::new(red_channel, green_channel, blue_channel);

    // Set initial color (blue - indicates not connected)
    rgb.set_color(0x000088).expect("Failed to set RGB color");

    println!("RGB LED initialized");

    // =========================================================================
    // Initialize drivers
    // =========================================================================

    // Create shift register and display
    let mut shift_register = ShiftRegister::new(&mut data_pin, &mut clock_pin, &mut latch_pin);
    let mut display = NixieDisplay::new(&mut shift_register, sep1, sep2);
    display.set_hour_format(HourFormat::TwelveHour);

    println!("Display initialized");

    // Create button debouncer with shared state
    let debouncer = DEBOUNCER.init(Mutex::new(Debouncer::new(0.1, 100, button_pin)));

    // Spawn debounce task
    spawner.spawn(debounce_task(debouncer)).ok();

    println!("Debouncer task spawned");

    // =========================================================================
    // Wait for WiFi connection and get IP (only when wifi feature is enabled)
    // =========================================================================

    #[cfg(feature = "wifi")]
    {
        loop {
            if stack.is_link_up() {
                break;
            }
            Timer::after(Duration::from_millis(500)).await;
        }

        println!("Waiting to get IP address...");
        loop {
            if let Some(config) = stack.config_v4() {
                println!("Got IP: {}", config.address);
                // Change LED to green to indicate connected
                rgb.set_color(0x008800).expect("Failed to set RGB color");
                break;
            }
            Timer::after(Duration::from_millis(500)).await;
        }

        println!("WiFi connected with IP, starting main loop");
    }

    // =========================================================================
    // Main display loop
    // =========================================================================

    // For now, use a fake time until NTP is implemented
    // This will increment every second to simulate time passing
    let mut fake_seconds: u32 = 12 * 3600 + 34 * 60; // Start at 12:34:00

    let mut button_hold_counter: u8 = 0;
    let mut ticker = Ticker::every(Duration::from_millis(200));

    #[cfg(feature = "wifi")]
    let mut last_wifi_status = true; // We started connected

    loop {
        ticker.next().await;

        // Update WiFi status LED (only when wifi feature is enabled)
        #[cfg(feature = "wifi")]
        {
            let current_wifi_status = stack.is_link_up();
            if current_wifi_status != last_wifi_status {
                if current_wifi_status {
                    rgb.set_color(0x008800).ok(); // Green = connected
                    println!("WiFi reconnected");
                } else {
                    rgb.set_color(0x880000).ok(); // Red = disconnected
                    println!("WiFi disconnected");
                }
                last_wifi_status = current_wifi_status;
            }
        }

        // Check button state for mode switching
        let button_pressed = {
            let mut guard = debouncer.lock().await;
            guard.is_low().unwrap_or(false)
        };

        if button_pressed {
            button_hold_counter = (button_hold_counter + 1) % 5;
        } else {
            button_hold_counter = 0;
        }

        // Switch mode on first detection of button press
        if button_hold_counter == 1 {
            display.next_mode();
            println!("Display mode changed");
        }

        // Create a NaiveDateTime for display
        // Using a fixed date since we don't have NTP yet
        let hours = (fake_seconds / 3600) % 24;
        let minutes = (fake_seconds / 60) % 60;
        let seconds = fake_seconds % 60;

        let datetime = NaiveDateTime::new(
            chrono::NaiveDate::from_ymd_opt(2024, 12, 27).unwrap(),
            chrono::NaiveTime::from_hms_opt(hours, minutes, seconds).unwrap(),
        );

        display.display(datetime);

        // Increment fake time (1 second per 5 ticks at 200ms = 1 second)
        fake_seconds += 1;

        // Only log every 5 seconds to reduce serial spam
        if seconds.is_multiple_of(5) && fake_seconds.is_multiple_of(5) {
            println!("Time: {:02}:{:02}:{:02}", hours, minutes, seconds);
        }
    }
}
