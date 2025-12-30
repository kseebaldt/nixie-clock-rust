#![no_std]
#![no_main]

use core::net::{IpAddr, SocketAddr};

use edge_nal::UdpBind;
use embassy_executor::Spawner;
use embassy_net::{
    Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4,
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
    ram,
    rng::Rng,
    rtc_cntl::Rtc,
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_println::println;
use esp_radio::wifi::{
    AccessPointConfig, ClientConfig, ModeConfig, WifiController, WifiDevice, WifiEvent,
};

use sntpc::{NtpContext, NtpTimestampGenerator, get_time};
use static_cell::StaticCell;

use chrono::{DateTime, Timelike};
use chrono_tz::Tz;
use drivers::config::InternalConfig;
use drivers::debouncer::Debouncer;
use drivers::nixie_display::{DisplayMode, HourFormat, NixieDisplay};
use drivers::rgb_led::RgbLed;
use drivers::shift_register::ShiftRegister;
use postcard::from_bytes;

use clock_no_std::storage::SeqConfigStorage;

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

// Type aliases for cleaner code
type ButtonDebouncer = Debouncer<Input<'static>>;
type DebouncerMutex = Mutex<CriticalSectionRawMutex, ButtonDebouncer>;
type DisplayModeMutex = Mutex<CriticalSectionRawMutex, DisplayMode>;
type ConfigMutex = Mutex<CriticalSectionRawMutex, InternalConfig>;

// Static storage for shared state
static DEBOUNCER: StaticCell<DebouncerMutex> = StaticCell::new();
static DISPLAY_MODE: StaticCell<DisplayModeMutex> = StaticCell::new();
static CONFIG: StaticCell<ConfigMutex> = StaticCell::new();

// Type alias for config storage mutex
type SeqConfigStorageMutex = Mutex<CriticalSectionRawMutex, SeqConfigStorage<'static>>;
static CONFIG_STORAGE: StaticCell<SeqConfigStorageMutex> = StaticCell::new();

// Watch for config changes - sends the actual config to all watchers
type ConfigWatch = embassy_sync::watch::Watch<CriticalSectionRawMutex, InternalConfig, 3>;
static CONFIG_WATCH: StaticCell<ConfigWatch> = StaticCell::new();

// NTP server to use for time sync
const NTP_SERVER: &str = "pool.ntp.org";

// Default timezone when parsing fails
const DEFAULT_TIMEZONE: Tz = chrono_tz::US::Eastern;

// HTTP server port
const HTTP_PORT: u16 = 8080;

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

/// Task to run the embassy-net network stack (pool_size=2 for AP and STA)
#[embassy_executor::task(pool_size = 2)]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

/// DHCP server task for AP mode - allows clients to get an IP address
#[embassy_executor::task]
async fn dhcp_server_task(stack: Stack<'static>) {
    use core::net::{Ipv4Addr, SocketAddrV4};
    use edge_dhcp::{
        io::{self, DEFAULT_SERVER_PORT},
        server::{Server, ServerOptions},
    };
    use edge_nal_embassy::{Udp, UdpBuffers};

    let ip = Ipv4Addr::new(192, 168, 4, 1);

    let mut buf = [0u8; 1500];
    let mut gw_buf = [Ipv4Addr::UNSPECIFIED];

    let buffers = UdpBuffers::<3, 1024, 1024, 10>::new();
    let unbound_socket = Udp::new(stack, &buffers);
    let mut bound_socket = match unbound_socket
        .bind(core::net::SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            DEFAULT_SERVER_PORT,
        )))
        .await
    {
        Ok(s) => s,
        Err(e) => {
            println!("DHCP: Failed to bind socket: {:?}", e);
            return;
        }
    };

    println!("DHCP: Server started on 192.168.4.1");

    loop {
        _ = io::server::run(
            &mut Server::<_, 64>::new_with_et(ip),
            &ServerOptions::new(ip, Some(&mut gw_buf)),
            &mut bound_socket,
            &mut buf,
        )
        .await
        .inspect_err(|e| println!("DHCP server error: {:?}", e));
        Timer::after(Duration::from_millis(500)).await;
    }
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

        // Perform NTP request using sntpc
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

                // Log UTC time
                let utc_secs = time.sec() as i64;
                let hours = ((utc_secs / 3600) % 24 + 24) % 24;
                let minutes = ((utc_secs / 60) % 60 + 60) % 60;
                let secs = (utc_secs % 60 + 60) % 60;

                println!(
                    "NTP: Time synced! UTC: {:02}:{:02}:{:02}",
                    hours, minutes, secs
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

/// Task to manage WiFi connection and reconnection (AP+STA mode)
/// Watches config and reconnects if WiFi credentials changed
#[embassy_executor::task]
async fn connection(
    mut controller: WifiController<'static>,
    mut config_receiver: embassy_sync::watch::DynReceiver<'static, InternalConfig>,
) {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.capabilities());

    // Track current WiFi credentials to detect changes
    let mut current_ssid = alloc::string::String::new();
    let mut current_pass = alloc::string::String::new();

    println!("Starting wifi (AP+STA mode)");
    controller.start_async().await.unwrap();
    println!("Wifi started!");

    loop {
        match esp_radio::wifi::ap_state() {
            esp_radio::wifi::WifiApState::Started => {
                println!("About to connect to STA...");
                match controller.connect_async().await {
                    Ok(_) => {
                        println!("STA connected!");
                        // Wait for either disconnection OR config change
                        use embassy_futures::select::{Either, select};
                        match select(
                            controller.wait_for_event(WifiEvent::StaDisconnected),
                            config_receiver.changed(),
                        )
                        .await
                        {
                            Either::First(_) => {
                                println!("STA disconnected");
                            }
                            Either::Second(new_config) => {
                                // Check if WiFi credentials changed
                                let new_ssid = alloc::string::String::from(new_config.wifi_ssid());
                                let new_pass = alloc::string::String::from(new_config.wifi_pass());

                                if new_ssid != current_ssid || new_pass != current_pass {
                                    println!("WiFi credentials changed, reconfiguring...");
                                    current_ssid = new_ssid.clone();
                                    current_pass = new_pass.clone();

                                    // Disconnect first
                                    let _ = controller.disconnect_async().await;

                                    // Reconfigure with new credentials
                                    let client_config = ModeConfig::ApSta(
                                        ClientConfig::default()
                                            .with_ssid(new_ssid)
                                            .with_password(new_pass),
                                        AccessPointConfig::default()
                                            .with_ssid("nixie-clock".into()),
                                    );
                                    controller.set_config(&client_config).unwrap();
                                    println!("WiFi reconfigured with new credentials");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!("Failed to connect to wifi: {e:?}");
                        Timer::after(Duration::from_millis(5000)).await
                    }
                }
            }
            _ => return,
        }
    }
}

/// Task to update RGB LED color when received via channel
/// This allows the HTTP handler to send color updates without owning the RGB hardware
#[embassy_executor::task]
async fn rgb_led_task(
    mut rgb: RgbLed<esp_hal::ledc::channel::Channel<'static, esp_hal::ledc::LowSpeed>>,
    mut config_receiver: embassy_sync::watch::DynReceiver<'static, InternalConfig>,
) {
    // Track current color to avoid unnecessary updates
    let mut current_color = 0u32;

    loop {
        // Wait for config change and get the new config
        let new_config = config_receiver.changed().await;
        let new_color = new_config.led_color();

        // Only update if color actually changed
        if new_color != current_color {
            current_color = new_color;
            if let Err(e) = rgb.set_color(new_color) {
                println!("RGB: Failed to set color: {:?}", e);
            } else {
                println!("RGB: Color updated to #{:06x}", new_color);
            }
        }
    }
}

/// GPIO pins for the display
struct DisplayPins {
    data: Output<'static>,
    clock: Output<'static>,
    latch: Output<'static>,
    sep1: Output<'static>,
    sep2: Output<'static>,
}

/// Parse timezone string to chrono_tz::Tz, falling back to default
fn parse_timezone(tz: &str) -> Tz {
    tz.parse().unwrap_or(DEFAULT_TIMEZONE)
}

/// Display task - updates the nixie display based on current time
#[embassy_executor::task]
async fn display_task(
    rtc: &'static Rtc<'static>,
    debouncer: &'static DebouncerMutex,
    display_mode: &'static DisplayModeMutex,
    config: &'static ConfigMutex,
    mut pins: DisplayPins,
) {
    let mut shift_register = ShiftRegister::new(&mut pins.data, &mut pins.clock, &mut pins.latch);
    let mut display = NixieDisplay::new(&mut shift_register, pins.sep1, pins.sep2);

    let mut button_hold_counter: u8 = 0;
    let mut ticker = Ticker::every(Duration::from_millis(200));
    let mut last_log_second: u32 = 0;

    println!("Display task started");

    loop {
        ticker.next().await;

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
            let mut mode = display_mode.lock().await;
            *mode = match *mode {
                DisplayMode::Time => DisplayMode::Date,
                DisplayMode::Date => DisplayMode::Year,
                DisplayMode::Year => DisplayMode::Time,
            };
            println!("Display mode changed to {:?}", *mode);
        }

        // Get current display mode, timezone, and hour format from config
        let (mode, tz, hours_24) = {
            let mode_guard = display_mode.lock().await;
            let config_guard = config.lock().await;
            (
                *mode_guard,
                parse_timezone(config_guard.tz()),
                config_guard.hours_24(),
            )
        };
        display.set_mode(mode);
        display.set_hour_format(if hours_24 {
            HourFormat::TwentyFourHour
        } else {
            HourFormat::TwelveHour
        });

        // Get current time from RTC (set by NTP sync task) as Unix timestamp
        let rtc_us = rtc.current_time_us();
        let unix_secs = (rtc_us / 1_000_000) as i64;

        // Convert UTC timestamp to local time using chrono-tz (handles DST automatically)
        let local_time = match DateTime::from_timestamp(unix_secs, 0) {
            Some(utc) => utc.with_timezone(&tz).naive_local(),
            None => {
                // Fallback if timestamp is invalid
                chrono::NaiveDateTime::default()
            }
        };

        display.display(local_time);

        // Log every 5 seconds
        let seconds = local_time.and_utc().timestamp() % 60;
        if seconds % 5 == 0 && seconds as u32 != last_log_second {
            println!(
                "Time: {:02}:{:02}:{:02}",
                local_time.time().hour(),
                local_time.time().minute(),
                local_time.time().second()
            );
            last_log_second = seconds as u32;
        }
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    // Initialize RTC for timekeeping (used with NTP)
    let rtc = {
        static RTC: StaticCell<Rtc<'static>> = StaticCell::new();
        &*RTC.init(Rtc::new(peripherals.LPWR))
    };

    println!("Embassy initialized!");

    // =========================================================================
    // Flash Storage & Config Setup
    // =========================================================================

    // Create the sequential storage instance
    let seq_storage = SeqConfigStorage::new(peripherals.FLASH);
    let config_storage = CONFIG_STORAGE.init(Mutex::new(seq_storage));

    // Load config from flash (async operation)
    let app_config = {
        let mut storage = config_storage.lock().await;
        let mut buf = [0u8; 256];
        match storage.load(&mut buf).await {
            Some(len) => {
                // Deserialize the config using postcard
                match from_bytes::<InternalConfig>(&buf[..len]) {
                    Ok(cfg) => {
                        println!("Config loaded from flash: {:?}", cfg);
                        cfg
                    }
                    Err(e) => {
                        println!("Failed to deserialize config: {:?}, using defaults", e);
                        InternalConfig::default()
                    }
                }
            }
            None => {
                // Flash may be corrupted or uninitialized - erase and start fresh
                println!("No config in flash or error, erasing storage area...");
                if let Err(e) = storage.erase_all().await {
                    println!("Failed to erase storage: {:?}", e);
                }
                println!("Using defaults");
                InternalConfig::default()
            }
        }
    };

    // Store config in static for sharing between tasks
    let config = CONFIG.init(Mutex::new(app_config.clone()));

    // Initialize config watch with current config - multiple tasks can subscribe
    let config_watch = CONFIG_WATCH.init(embassy_sync::watch::Watch::new());
    config_watch.sender().send(app_config.clone());

    // =========================================================================
    // WiFi Setup (AP + STA mode for both internet access and local server)
    // =========================================================================

    let (ap_stack, sta_stack) = {
        println!("Initializing WiFi...");
        let esp_radio_ctrl =
            &*mk_static!(esp_radio::Controller<'static>, esp_radio::init().unwrap());
        let (mut controller, interfaces) =
            esp_radio::wifi::new(esp_radio_ctrl, peripherals.WIFI, Default::default()).unwrap();

        let wifi_ap_device = interfaces.ap;
        let wifi_sta_device = interfaces.sta;

        println!("WiFi controller initialized");

        // AP config with static IP
        let ap_config = embassy_net::Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(core::net::Ipv4Addr::new(192, 168, 4, 1), 24),
            gateway: Some(core::net::Ipv4Addr::new(192, 168, 4, 1)),
            dns_servers: Default::default(),
        });

        // STA config with DHCP
        let sta_config = embassy_net::Config::dhcpv4(Default::default());

        let rng = Rng::new();
        let seed = (rng.random() as u64) << 32 | rng.random() as u64;

        // Create both network stacks
        let (ap_stack, ap_runner) = embassy_net::new(
            wifi_ap_device,
            ap_config,
            mk_static!(StackResources<3>, StackResources::<3>::new()),
            seed,
        );
        let (sta_stack, sta_runner) = embassy_net::new(
            wifi_sta_device,
            sta_config,
            mk_static!(StackResources<5>, StackResources::<5>::new()),
            seed,
        );

        // Configure AP+STA mode using credentials from config
        println!("WiFi STA credentials: SSID='{}'", app_config.wifi_ssid());

        let client_config = ModeConfig::ApSta(
            ClientConfig::default()
                .with_ssid(app_config.wifi_ssid().into())
                .with_password(app_config.wifi_pass().into()),
            AccessPointConfig::default().with_ssid("nixie-clock".into()),
        );
        controller.set_config(&client_config).unwrap();

        // Spawn network tasks
        spawner
            .spawn(connection(controller, config_watch.dyn_receiver().unwrap()))
            .ok();
        spawner.spawn(net_task(ap_runner)).ok();
        spawner.spawn(net_task(sta_runner)).ok();

        println!("Network tasks spawned (AP+STA mode)");
        (ap_stack, sta_stack)
    };

    // Spawn DHCP server for AP mode (so clients can get an IP)
    spawner.spawn(dhcp_server_task(ap_stack)).ok();

    // =========================================================================
    // GPIO Setup
    // =========================================================================

    // Shift register pins
    let data_pin = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    let clock_pin = Output::new(peripherals.GPIO17, Level::Low, OutputConfig::default());
    let latch_pin = Output::new(peripherals.GPIO18, Level::Low, OutputConfig::default());

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

    let lstimer0 = mk_static!(
        esp_hal::ledc::timer::Timer<'static, LowSpeed>,
        ledc.timer::<LowSpeed>(timer::Number::Timer0)
    );
    lstimer0
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty8Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(5),
        })
        .expect("Failed to configure LEDC timer");

    let mut red_channel = ledc.channel(channel::Number::Channel0, peripherals.GPIO27);
    red_channel
        .configure(channel::config::Config {
            timer: lstimer0,
            duty_pct: 100,
            drive_mode: DriveMode::PushPull,
        })
        .expect("Failed to configure red channel");

    let mut green_channel = ledc.channel(channel::Number::Channel1, peripherals.GPIO26);
    green_channel
        .configure(channel::config::Config {
            timer: lstimer0,
            duty_pct: 100,
            drive_mode: DriveMode::PushPull,
        })
        .expect("Failed to configure green channel");

    let mut blue_channel = ledc.channel(channel::Number::Channel2, peripherals.GPIO25);
    blue_channel
        .configure(channel::config::Config {
            timer: lstimer0,
            duty_pct: 100,
            drive_mode: DriveMode::PushPull,
        })
        .expect("Failed to configure blue channel");

    let mut rgb = RgbLed::new(red_channel, green_channel, blue_channel);
    rgb.set_color(app_config.led_color())
        .expect("Failed to set RGB color");

    println!(
        "RGB LED initialized with color: #{:06x}",
        app_config.led_color()
    );

    // Spawn RGB LED task with config watch receiver
    spawner
        .spawn(rgb_led_task(rgb, config_watch.dyn_receiver().unwrap()))
        .ok();
    println!("RGB LED task spawned");

    // =========================================================================
    // Initialize shared state
    // =========================================================================

    let debouncer = DEBOUNCER.init(Mutex::new(Debouncer::new(0.1, 100, button_pin)));
    let display_mode = DISPLAY_MODE.init(Mutex::new(DisplayMode::Time));

    // Spawn debounce task
    spawner.spawn(debounce_task(debouncer)).ok();
    println!("Debouncer task spawned");

    // Spawn display task
    let display_pins = DisplayPins {
        data: data_pin,
        clock: clock_pin,
        latch: latch_pin,
        sep1,
        sep2,
    };
    spawner
        .spawn(display_task(
            rtc,
            debouncer,
            display_mode,
            config,
            display_pins,
        ))
        .ok();
    println!("Display task spawned");

    // =========================================================================
    // Wait for WiFi connection (both AP and STA)
    // =========================================================================

    // Wait for AP to be ready (should be immediate with static IP)
    println!("Waiting for AP interface...");
    loop {
        let ap_state = esp_radio::wifi::ap_state();
        println!(
            "AP state: {:?}, link_up: {}, config_up: {}",
            ap_state,
            ap_stack.is_link_up(),
            ap_stack.is_config_up()
        );
        if ap_stack.is_link_up() && ap_stack.is_config_up() {
            if let Some(config) = ap_stack.config_v4() {
                println!("AP ready at: {}", config.address);
            }
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    // Wait for STA to connect and get DHCP
    println!("Waiting for STA to connect...");
    loop {
        if sta_stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("STA connected, waiting for DHCP...");
    loop {
        if let Some(cfg) = sta_stack.config_v4() {
            println!("STA got IP: {}", cfg.address);
            // Send current config to trigger RGB task to set the configured color
            config_watch.sender().send(app_config.clone());
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    // Wait for STA network config to be fully up
    println!("Waiting for STA network config to be up...");
    while !sta_stack.is_config_up() {
        Timer::after(Duration::from_millis(100)).await;
    }
    println!("STA network config is up!");

    println!("WiFi connected with IP, starting services");

    // Spawn NTP sync task (uses STA stack for internet access)
    spawner.spawn(ntp_sync_task(sta_stack, rtc)).ok();

    // =========================================================================
    // HTTP Server (runs in main)
    // =========================================================================

    // Test outbound TCP connection first (using STA stack)
    println!("Testing outbound TCP connection via STA...");
    {
        use core::net::Ipv4Addr;
        use embassy_net::tcp::TcpSocket;
        use embedded_io_async::Write;

        let mut rx_buffer = [0; 1024];
        let mut tx_buffer = [0; 1024];
        let mut socket = TcpSocket::new(sta_stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));

        // Try to connect to a known server (Google)
        let remote = (Ipv4Addr::new(142, 250, 185, 115), 80);
        println!("Connecting to {:?}...", remote);
        match socket.connect(remote).await {
            Ok(_) => {
                println!("Connected! TCP outbound works.");
                let _ = socket
                    .write_all(b"GET / HTTP/1.0\r\nHost: www.google.com\r\n\r\n")
                    .await;
                let mut buf = [0; 256];
                if let Ok(n) = socket.read(&mut buf).await {
                    println!("Got {} bytes response", n);
                }
            }
            Err(e) => println!("Connect failed: {:?}", e),
        }
        socket.abort();
    }

    // Print connection info
    println!("HTTP: Starting web server on port {}", HTTP_PORT);
    println!(
        "HTTP: Connect to 'nixie-clock' WiFi and browse to http://192.168.4.1:{}",
        HTTP_PORT
    );
    if let Some(cfg) = sta_stack.config_v4() {
        println!(
            "HTTP: Or use your home network and browse to http://{}:{}",
            cfg.address.address(),
            HTTP_PORT
        );
    }

    // Create picoserve app using AppWithStateBuilder pattern
    use clock_no_std::http::{AppProps, AppState, server_config};
    use picoserve::{AppRouter, AppWithStateBuilder};

    let app: &'static AppRouter<AppProps> = mk_static!(AppRouter<AppProps>, AppProps.build_app());
    let server_cfg = mk_static!(picoserve::Config<Duration>, server_config());

    let app_state: &'static AppState = mk_static!(
        AppState,
        AppState {
            config_storage,
            config_sender: config_watch.sender(),
            current_config: config,
        }
    );

    // Run HTTP servers on both AP and STA interfaces concurrently
    let mut ap_rx = [0; 2048];
    let mut ap_tx = [0; 2048];
    let mut sta_rx = [0; 2048];
    let mut sta_tx = [0; 2048];
    let mut ap_http = [0; 2048];
    let mut sta_http = [0; 2048];

    println!("HTTP: Listening on AP and STA interfaces...");

    // Use select to run both servers - whichever gets a connection first handles it
    loop {
        embassy_futures::select::select(
            picoserve::Server::new(
                &app.shared().with_state(app_state),
                server_cfg,
                &mut ap_http,
            )
            .listen_and_serve("AP", ap_stack, HTTP_PORT, &mut ap_rx, &mut ap_tx),
            picoserve::Server::new(
                &app.shared().with_state(app_state),
                server_cfg,
                &mut sta_http,
            )
            .listen_and_serve("STA", sta_stack, HTTP_PORT, &mut sta_rx, &mut sta_tx),
        )
        .await;
    }
}
