use esp_idf_svc::sntp;
use esp_idf_svc::sys::EspError;

use chrono::Utc;
use chrono_tz::Tz;

use drivers::{nixie_display::NixieDisplay, shift_register::ShiftRegister};
use esp_idf_svc::hal::{gpio::*, modem::Modem, prelude::*};

#[toml_cfg::toml_config]
pub struct Config {
    #[default("Wokwi-GUEST")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_pass: &'static str,
}

use log::info;

fn main() -> Result<(), EspError> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;
    let modem = peripherals.modem;

    // Create the shift register
    let mut data_pin = PinDriver::output(pins.gpio16)?;
    let mut clock_pin = PinDriver::output(pins.gpio17)?;
    let mut latch_pin = PinDriver::output(pins.gpio18)?;
    let mut seperator1 = PinDriver::output(pins.gpio4)?;
    let mut seperator2 = PinDriver::output(pins.gpio2)?;

    let mut sr = ShiftRegister::new(&mut data_pin, &mut clock_pin, &mut latch_pin);

    let mut display = NixieDisplay::new(&mut sr, &mut seperator1, &mut seperator2);

    // Keep it around or else the wifi will stop
    let _wifi = wifi_create(modem)?;

    // Keep it around or else the SNTP service will stop
    let _sntp = sntp::EspSntp::new_default()?;
    info!("SNTP initialized");

    let tz: Tz = "America/Chicago".parse().unwrap();
    info!("Time Zone: {:?}", tz);

    loop {
        // To get a better formatting of the time, you can use the `chrono` or `time` Rust crates
        let local_time = Utc::now().with_timezone(&tz);
        info!("Current time: {:?}", local_time);

        display.display(local_time);

        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

fn wifi_create(modem: Modem) -> Result<esp_idf_svc::wifi::EspWifi<'static>, EspError> {
    use esp_idf_svc::eventloop::*;
    use esp_idf_svc::nvs::*;
    use esp_idf_svc::wifi::*;

    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut esp_wifi = EspWifi::new(modem, sys_loop.clone(), Some(nvs.clone()))?;
    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sys_loop.clone())?;

    let app_config = CONFIG;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: app_config.wifi_ssid.try_into().unwrap(),
        password: app_config.wifi_pass.try_into().unwrap(),
        auth_method: AuthMethod::None,
        ..Default::default()
    }))?;

    wifi.start()?;
    info!("Wifi started");

    wifi.connect()?;
    info!("Wifi connected");

    wifi.wait_netif_up()?;
    info!("Wifi netif up");

    Ok(esp_wifi)
}
