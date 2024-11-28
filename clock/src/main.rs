use drivers::nixie_display::HourFormat;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use esp_idf_svc::{eventloop::EspSystemEventLoop, sntp, timer::EspTaskTimerService};
use std::{
    sync::{mpsc::channel, Arc, Mutex},
    time::Duration,
};

use chrono::Utc;
use chrono_tz::Tz;

use drivers::{
    config::{ConfigStorage, InternalConfig, DEFAULT_CONFIG},
    debouncer::Debouncer,
    nixie_display::NixieDisplay,
    rgb_led::RgbLed,
    shift_register::ShiftRegister,
    storage::{InMemoryStorage, Storage},
};
use embedded_hal::digital::InputPin;
use esp_idf_svc::hal::{gpio::*, prelude::*};
use esp_idf_svc::nvs::{EspCustomNvsPartition, EspDefaultNvsPartition};
use nixie_clock_rust::storage::NvsStorage;

use log::info;
use nixie_clock_rust::rgb_led::create_driver;
use nixie_clock_rust::server::create_server;
use nixie_clock_rust::wifi::configure_wifi;

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
        create_driver(ledc.channel0, ledc.timer0, pins.gpio27)?,
        create_driver(ledc.channel1, ledc.timer1, pins.gpio26)?,
        create_driver(ledc.channel2, ledc.timer2, pins.gpio25)?,
    );

    let button_debouncer = Arc::new(Mutex::new(Debouncer::new(
        0.1,
        100,
        PinDriver::input(pins.gpio19)?,
    )));

    let timer_service = EspTaskTimerService::new()?;
    let callback_timer = {
        let button_debouncer = button_debouncer.clone();
        timer_service.timer(move || {
            button_debouncer.lock().unwrap().update().unwrap();
        })?
    };

    // Let it trigger every second
    callback_timer.every(Duration::from_millis(10))?;

    let app_config = config_storage.lock().unwrap().load()?;
    info!("Setting led color to: #{:06x}", app_config.led_color());
    let hour_format = if app_config.hours_24() { HourFormat::TwentyFourHour } else { HourFormat::TwelveHour };
    display.set_hour_format(hour_format);
    rgb.set_color(app_config.led_color())?;

    let default_config = DEFAULT_CONFIG;
    // Keep it around or else the wifi will stop
    let sys_loop: esp_idf_svc::eventloop::EspEventLoop<esp_idf_svc::eventloop::System> =
        EspSystemEventLoop::take()?;

    let nvs = EspDefaultNvsPartition::take()?;

    let mut esp_wifi = EspWifi::new(modem, sys_loop.clone(), Some(nvs.clone()))?;
    let mut wifi: BlockingWifi<&mut EspWifi<'_>> =
        BlockingWifi::wrap(&mut esp_wifi, sys_loop.clone())?;

    if let Err(e) = configure_wifi(&mut wifi, &app_config, &default_config) {
        info!("Error configuring wifi: {:?}", e);
    }
    // Keep it around or else the SNTP service will stop
    let mut _sntp = sntp::EspSntp::new_default()?;
    info!("SNTP initialized");
    let (tx, rx) = channel::<InternalConfig>();
    let mut _server = create_server(config_storage.clone(), tx.clone())?;

    let mut tz: Tz = app_config.tz().parse().unwrap();
    info!("Time Zone: {:?}", tz);

    let mut counter = 0;
    loop {
        if let Ok(config) = rx.try_recv() {
            info!("Received new config: {:?}", config);
            rgb.set_color(config.led_color())?;
            tz = config.tz().parse().unwrap();

            let wifi_config = wifi.get_configuration()?;
            let client_config = wifi_config.as_client_conf_ref().unwrap();
            if client_config.ssid != config.wifi_ssid()
                || client_config.password != config.wifi_pass()
            {
                wifi.stop()?;

                if let Err(e) = configure_wifi(&mut wifi, &config, &default_config) {
                    info!("Error configuring wifi: {:?}", e);
                } else {
                    drop(_sntp);
                    _sntp = sntp::EspSntp::new_default()?;
                }
            }
        }

        if button_debouncer.lock().unwrap().is_low().unwrap() {
            counter = (counter + 1) % 5;
        } else {
            counter = 0;
        }

        if counter == 1 {
            display.next_mode();
        }

        // To get a better formatting of the time, you can use the `chrono` or `time` Rust crates
        let local_time = Utc::now().with_timezone(&tz);
        info!(
            "Current time: {:?} -- Button: {}",
            local_time,
            button_debouncer.lock().unwrap().is_low().unwrap()
        );
        display.display(local_time);

        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
