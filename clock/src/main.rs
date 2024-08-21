use esp_idf_svc::sntp;
use std::sync::{Arc, Mutex};

use anyhow::Result;

use chrono::Utc;
use chrono_tz::Tz;

use embedded_svc::{
    http::{Headers, Method},
    io::{Read, Write},
};

use drivers::{
    nixie_display::NixieDisplay,
    shift_register::ShiftRegister,
    storage::{InMemoryStorage, Storage},
};
use esp_idf_svc::hal::{gpio::*, modem::Modem, prelude::*};
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::nvs::EspCustomNvsPartition;
use nixie_clock_rust::storage::NvsStorage;
use postcard::{from_bytes, to_vec};
use serde::{Deserialize, Serialize};

const MAX_LEN: usize = 256;

#[toml_cfg::toml_config]
pub struct WifiConfig {
    #[default("Wokwi-GUEST")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_pass: &'static str,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Config<'a> {
    wifi_ssid: &'a str,
    wifi_pass: &'a str,
    tz: &'a str,
    led_color: u32,
}

impl Default for Config<'_> {
    fn default() -> Self {
        let wifi_config = WIFI_CONFIG;
        Config {
            wifi_ssid: &wifi_config.wifi_ssid,
            wifi_pass: &wifi_config.wifi_pass,
            tz: "America/Chicago",
            led_color: 0x00000088,
        }
    }
}

use log::info;
use nixie_clock_rust::rgb_led::RgbLed;

const STACK_SIZE: usize = 10240;
static INDEX_HTML: &str = include_str!("../../webapp/dist/index.html");

fn main() -> Result<()> {
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

    let storage: Arc<Mutex<dyn Storage + Send>> = match EspCustomNvsPartition::take("config") {
        Ok(partition) => Arc::new(Mutex::new(NvsStorage::new(partition, "storage")?)),
        Err(_) => Arc::new(Mutex::new(InMemoryStorage::new())),
    };

    // Create the shift register
    let mut data_pin = PinDriver::output(pins.gpio16)?;
    let mut clock_pin = PinDriver::output(pins.gpio17)?;
    let mut latch_pin = PinDriver::output(pins.gpio18)?;
    let mut seperator1 = PinDriver::output(pins.gpio4)?;
    let mut seperator2 = PinDriver::output(pins.gpio2)?;

    let mut sr = ShiftRegister::new(&mut data_pin, &mut clock_pin, &mut latch_pin);

    let mut display = NixieDisplay::new(&mut sr, &mut seperator1, &mut seperator2);

    let mut rgb = RgbLed::new(
        ledc.channel0,
        ledc.timer0,
        pins.gpio25,
        ledc.channel1,
        ledc.timer1,
        pins.gpio26,
        ledc.channel2,
        ledc.timer2,
        pins.gpio27,
    )?;

    rgb.set_color(0x00000088)?;

    // Keep it around or else the wifi will stop
    let app_config = Config::default();
    let _wifi = wifi_create(modem, &app_config)?;

    // Keep it around or else the SNTP service will stop
    let _sntp = sntp::EspSntp::new_default()?;
    info!("SNTP initialized");

    let tz: Tz = "America/Chicago".parse().unwrap();
    info!("Time Zone: {:?}", tz);

    let server_configuration = esp_idf_svc::http::server::Configuration {
        stack_size: STACK_SIZE,
        ..Default::default()
    };

    let mut server = EspHttpServer::new(&server_configuration)?;

    server.fn_handler("/", Method::Get, |req| {
        req.into_ok_response()?
            .write_all(INDEX_HTML.as_bytes())
            .map(|_| ())
    })?;

    let storage2 = storage.clone();
    server.fn_handler("/config", Method::Get, move |req| {
        let mut buf: [u8; MAX_LEN] = [0; MAX_LEN];
        let s = storage2.lock().unwrap();
        let config = match s.get_raw("config", &mut buf) {
            Ok(Some(v)) => from_bytes::<Config>(v).unwrap(),
            _ => Config::default(),
        };
        let j = serde_json::to_string(&config).unwrap();

        req.into_response(200, Some("OK"), &[("Content-Type", "application/json")])?
            .write_all(j.as_bytes())
            .map(|_| ())
    })?;

    let storage3 = storage.clone();
    server.fn_handler::<anyhow::Error, _>("/config", Method::Post, move |mut req| {
        let len = req.content_len().unwrap_or(0) as usize;
        let mut s = storage3.lock().unwrap();

        if len > MAX_LEN {
            req.into_status_response(413)?
                .write_all("Request too big".as_bytes())?;
            return Ok(());
        }

        let mut buf = vec![0; len];
        req.read_exact(&mut buf)?;

        if let Ok(config) = serde_json::from_slice::<Config>(&buf) {
            info!("Config: {:?}", config);

            match s.set_raw(
                "config",
                &to_vec::<Config, 100>(&config).unwrap(),
            ) {
                Ok(_) => {
                    req.into_response(200, Some("OK"), &[("Content-Type", "application/json")])?
                    .write_all("{{\"status\":\"ok\"}}".as_bytes())?;    
                },
                Err(_) => {
                    req.into_status_response(500)?
                    .write_all("JSON error".as_bytes())?;
                },
            };
        } else {
            req.into_status_response(500)?
                .write_all("JSON error".as_bytes())?;
        }

        Ok(())
    })?;

    loop {
        // To get a better formatting of the time, you can use the `chrono` or `time` Rust crates
        let local_time = Utc::now().with_timezone(&tz);
        info!("Current time: {:?}", local_time);

        display.display(local_time);

        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

fn wifi_create(modem: Modem, app_config: &Config) -> Result<esp_idf_svc::wifi::EspWifi<'static>> {
    use esp_idf_svc::eventloop::*;
    use esp_idf_svc::nvs::*;
    use esp_idf_svc::wifi::*;

    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut esp_wifi = EspWifi::new(modem, sys_loop.clone(), Some(nvs.clone()))?;
    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sys_loop.clone())?;

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
