#![no_std]
#![no_main]

use core::net::{IpAddr, SocketAddr};

use embassy_executor::Spawner;
use embassy_net::{
    Runner, Stack, StackResources,
    dns::DnsQueryType,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker, Timer};
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
    rng::Rng,
    rtc_cntl::Rtc,
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_println::println;
use esp_radio::wifi::{
    ClientConfig, ModeConfig, ScanConfig, WifiController, WifiDevice, WifiEvent, WifiStaState,
};
use sntpc::{NtpContext, NtpTimestampGenerator, get_time};
use static_cell::StaticCell;

use chrono::NaiveDateTime;
use drivers::debouncer::Debouncer;
use drivers::nixie_display::{HourFormat, NixieDisplay};
use drivers::rgb_led::RgbLed;
use drivers::shift_register::ShiftRegister;

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

// WiFi credentials - set via environment variables at build time
// Build with: WIFI_SSID="YourNetwork" WIFI_PASSWORD="YourPassword" cargo build --release
const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

// Type aliases for cleaner code
type ButtonDebouncer = Debouncer<Input<'static>>;
type DebouncerMutex = Mutex<CriticalSectionRawMutex, ButtonDebouncer>;

// Static storage for shared state
static DEBOUNCER: StaticCell<DebouncerMutex> = StaticCell::new();

// NTP server to use for time sync
const NTP_SERVER: &str = "pool.ntp.org";

// Timezone offset in seconds (e.g., -5 hours for EST = -18000)
// TODO: Make this configurable
const TIMEZONE_OFFSET_SECS: i64 = -5 * 3600; // EST

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
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
#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

/// Timestamp generator for sntpc using the ESP32 RTC
#[derive(Clone, Copy)]
struct RtcTimestamp {
    current_time_us: u64,
}

impl NtpTimestampGenerator for RtcTimestamp {
    fn init(&mut self) {
        // We'll set this before each NTP request
    }

    fn timestamp_sec(&self) -> u64 {
        self.current_time_us / 1_000_000
    }

    fn timestamp_subsec_micros(&self) -> u32 {
        (self.current_time_us % 1_000_000) as u32
    }
}

/// Task to periodically sync time via NTP
#[embassy_executor::task]
async fn ntp_sync_task(stack: Stack<'static>, rtc: &'static Rtc<'static>) {
    // Wait for network to be ready
    loop {
        if stack.is_link_up() && stack.config_v4().is_some() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("NTP: Starting time sync task");

    // UDP socket buffers
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx_buffer = [0; 512];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_buffer = [0; 512];

    loop {
        // Resolve NTP server address
        println!("NTP: Resolving {}...", NTP_SERVER);
        let ntp_addrs = match stack.dns_query(NTP_SERVER, DnsQueryType::A).await {
            Ok(addrs) if !addrs.is_empty() => addrs,
            Ok(_) => {
                println!("NTP: DNS returned empty result");
                Timer::after(Duration::from_secs(30)).await;
                continue;
            }
            Err(e) => {
                println!("NTP: DNS error: {:?}", e);
                Timer::after(Duration::from_secs(30)).await;
                continue;
            }
        };

        let addr: IpAddr = ntp_addrs[0].into();
        println!("NTP: Resolved to {}", addr);

        // Create UDP socket
        let mut socket = UdpSocket::new(
            stack,
            &mut rx_meta,
            &mut rx_buffer,
            &mut tx_meta,
            &mut tx_buffer,
        );

        if let Err(e) = socket.bind(0) {
            println!("NTP: Failed to bind socket: {:?}", e);
            Timer::after(Duration::from_secs(30)).await;
            continue;
        }

        // Perform NTP request
        let context = NtpContext::new(RtcTimestamp {
            current_time_us: rtc.current_time_us(),
        });

        match get_time(SocketAddr::from((addr, 123)), &socket, context).await {
            Ok(time) => {
                // Convert NTP time to microseconds and set RTC
                const USEC_IN_SEC: u64 = 1_000_000;
                let time_us = (time.sec() as u64 * USEC_IN_SEC)
                    + ((time.sec_fraction() as u64 * USEC_IN_SEC) >> 32);
                rtc.set_current_time_us(time_us);

                // Calculate local time for display
                let local_secs = time.sec() as i64 + TIMEZONE_OFFSET_SECS;
                let hours = ((local_secs / 3600) % 24 + 24) % 24;
                let minutes = (local_secs / 60) % 60;
                let secs = local_secs % 60;

                println!(
                    "NTP: Time synced! UTC: {} Local: {:02}:{:02}:{:02}",
                    time.sec(),
                    hours,
                    minutes,
                    secs
                );
            }
            Err(e) => {
                println!("NTP: Error getting time: {:?}", e);
            }
        }

        // Sync every 10 minutes
        Timer::after(Duration::from_secs(600)).await;
    }
}

/// Task to manage WiFi connection and reconnection
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

    // Initialize RTC for timekeeping (used with NTP)
    let rtc = {
        static RTC: StaticCell<Rtc<'static>> = StaticCell::new();
        &*RTC.init(Rtc::new(peripherals.LPWR))
    };

    println!("Embassy initialized!");

    // =========================================================================
    // WiFi Setup
    // =========================================================================

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
    // Wait for WiFi connection and get IP
    // =========================================================================

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

    println!("WiFi connected with IP, starting NTP sync");

    // Spawn NTP sync task
    spawner.spawn(ntp_sync_task(stack, rtc)).ok();

    // =========================================================================
    // Main display loop
    // =========================================================================

    let mut button_hold_counter: u8 = 0;
    let mut ticker = Ticker::every(Duration::from_millis(200));
    let mut last_log_second: u32 = 0;
    let mut last_wifi_status = true; // We started connected

    println!("Starting main loop");

    loop {
        ticker.next().await;

        // Update WiFi status LED
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

        // Get current time from RTC (set by NTP sync task)
        let rtc_us = rtc.current_time_us();
        let total_secs = (rtc_us / 1_000_000) as i64 + TIMEZONE_OFFSET_SECS;

        // Convert Unix timestamp to date/time components
        // Days since Unix epoch (1970-01-01)
        let days = total_secs / 86400;
        let time_of_day = ((total_secs % 86400) + 86400) % 86400; // Handle negative

        let hours = (time_of_day / 3600) as u32;
        let minutes = ((time_of_day % 3600) / 60) as u32;
        let seconds = (time_of_day % 60) as u32;

        // Simple date calculation (good enough for display)
        let (year, month, day) = days_to_ymd(days as i32);

        // Create NaiveDateTime for display
        let datetime = NaiveDateTime::new(
            chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap_or_else(|| {
                chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
            }),
            chrono::NaiveTime::from_hms_opt(hours, minutes, seconds).unwrap_or_else(|| {
                chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()
            }),
        );

        display.display(datetime);

        // Log every 5 seconds to reduce serial spam
        if seconds.is_multiple_of(5) && seconds != last_log_second {
            println!("Time: {:02}:{:02}:{:02}", hours, minutes, seconds);
            last_log_second = seconds;
        }
    }
}

/// Convert days since Unix epoch to year/month/day
/// This is a simplified algorithm that works for dates from 1970 onwards
fn days_to_ymd(days: i32) -> (i32, u32, u32) {
    // Algorithm based on Howard Hinnant's date algorithms
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
