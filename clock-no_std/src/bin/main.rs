#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker};
use embedded_hal::digital::InputPin;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{DriveMode, Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::ledc::channel::{self, ChannelIFace};
use esp_hal::ledc::timer::{self, TimerIFace};
use esp_hal::ledc::{LSGlobalClkSource, Ledc, LowSpeed};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use static_cell::StaticCell;
use {esp_backtrace as _, esp_println as _};

use chrono::NaiveDateTime;
use drivers::debouncer::Debouncer;
use drivers::nixie_display::{HourFormat, NixieDisplay};
use drivers::rgb_led::RgbLed;
use drivers::shift_register::ShiftRegister;

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

// Type aliases for cleaner code
type ButtonDebouncer = Debouncer<Input<'static>>;
type DebouncerMutex = Mutex<CriticalSectionRawMutex, ButtonDebouncer>;

// Static storage for shared state
static DEBOUNCER: StaticCell<DebouncerMutex> = StaticCell::new();

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

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    // Initialize WiFi (keep for later phases)
    let _radio_init = esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller");
    let (_wifi_controller, _interfaces) =
        esp_radio::wifi::new(&_radio_init, peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

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

    info!("GPIO configured");

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

    // Set initial color (blue)
    rgb.set_color(0x000088).expect("Failed to set RGB color");

    info!("RGB LED initialized");

    // =========================================================================
    // Initialize drivers
    // =========================================================================

    // Create shift register and display
    let mut shift_register = ShiftRegister::new(&mut data_pin, &mut clock_pin, &mut latch_pin);
    let mut display = NixieDisplay::new(&mut shift_register, sep1, sep2);
    display.set_hour_format(HourFormat::TwelveHour);

    info!("Display initialized");

    // Create button debouncer with shared state
    let debouncer = DEBOUNCER.init(Mutex::new(Debouncer::new(0.1, 100, button_pin)));

    // Spawn debounce task
    spawner.spawn(debounce_task(debouncer)).ok();

    info!("Debouncer task spawned");

    // =========================================================================
    // Main display loop
    // =========================================================================

    // For now, use a fake time until NTP is implemented
    // This will increment every second to simulate time passing
    let mut fake_seconds: u32 = 12 * 3600 + 34 * 60; // Start at 12:34:00

    let mut button_hold_counter: u8 = 0;
    let mut ticker = Ticker::every(Duration::from_millis(200));

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
            display.next_mode();
            info!("Display mode changed");
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

        info!(
            "Time: {:02}:{:02}:{:02} | Button: {}",
            hours, minutes, seconds, button_pressed
        );
    }
}
