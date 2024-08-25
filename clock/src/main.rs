use esp_idf_svc::sntp;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use chrono_tz::Tz;

use drivers::{
    config::ConfigStorage,
    nixie_display::NixieDisplay,
    shift_register::ShiftRegister,
    storage::{InMemoryStorage, Storage},
};
use esp_idf_svc::hal::{gpio::*, prelude::*};
use esp_idf_svc::nvs::EspCustomNvsPartition;
use nixie_clock_rust::storage::NvsStorage;

use log::info;
use nixie_clock_rust::rgb_led::RgbLed;
use nixie_clock_rust::server::create_server;
use nixie_clock_rust::wifi::wifi_create;

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;
    let modem = peripherals.modem;
    let ledc = peripherals.ledc;

    let storage: Box<dyn Storage + Send> = match EspCustomNvsPartition::take("config") {
        Ok(partition) => Box::new(NvsStorage::new(partition, "storage")?),
        Err(_) => Box::new(InMemoryStorage::new()),
    };

    let config_storage = Arc::new(Mutex::new(ConfigStorage::new(storage)));

    // Create the shift register
    let mut data_pin = PinDriver::output(pins.gpio16)?;
    let mut clock_pin = PinDriver::output(pins.gpio17)?;
    let mut latch_pin = PinDriver::output(pins.gpio18)?;
    let mut seperator1 = PinDriver::output(pins.gpio4)?;
    let mut seperator2 = PinDriver::output(pins.gpio2)?;

    let mut shift_register = ShiftRegister::new(&mut data_pin, &mut clock_pin, &mut latch_pin);
    let mut display = NixieDisplay::new(&mut shift_register, &mut seperator1, &mut seperator2);

    let mut rgb = RgbLed::new(
        RgbLed::create_driver(ledc.channel0, ledc.timer0, pins.gpio25)?,
        RgbLed::create_driver(ledc.channel1, ledc.timer1, pins.gpio26)?,
        RgbLed::create_driver(ledc.channel2, ledc.timer2, pins.gpio27)?,
    )?;

    rgb.set_color(0x00000088)?;

    // Keep it around or else the wifi will stop
    let app_config = config_storage.lock().unwrap().load()?;
    let _wifi = wifi_create(modem, &app_config)?;

    // Keep it around or else the SNTP service will stop
    let _sntp = sntp::EspSntp::new_default()?;
    info!("SNTP initialized");

    let tz: Tz = "America/Chicago".parse().unwrap();
    info!("Time Zone: {:?}", tz);

    let _server = create_server(config_storage)?;

    loop {
        // To get a better formatting of the time, you can use the `chrono` or `time` Rust crates
        let local_time = Utc::now().with_timezone(&tz);
        info!("Current time: {:?}", local_time);

        display.display(local_time);

        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
