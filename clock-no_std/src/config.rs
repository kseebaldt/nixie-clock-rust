extern crate alloc;
use alloc::{string::String, format, vec::Vec};
use serde::{Deserialize, Serialize};
use postcard::{from_bytes, to_vec};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct InternalConfig {
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub tz: String,
    pub led_color: u32,
    pub hours_24: bool,
}

impl Default for InternalConfig {
    fn default() -> Self {
        InternalConfig {
            wifi_ssid: String::from("Wokwi-GUEST"),
            wifi_pass: String::from(""),
            tz: String::from("US/Central"),
            led_color: 0x00000088,
            hours_24: false,
        }
    }
}

impl InternalConfig {
    pub fn new(wifi_ssid: &str, wifi_pass: &str, tz: &str, led_color: u32, hours_24: bool) -> Self {
        InternalConfig {
            wifi_ssid: String::from(wifi_ssid),
            wifi_pass: String::from(wifi_pass),
            tz: String::from(tz),
            led_color,
            hours_24,
        }
    }

    pub fn wifi_ssid(&self) -> &str {
        &self.wifi_ssid
    }

    pub fn wifi_pass(&self) -> &str {
        &self.wifi_pass
    }

    pub fn tz(&self) -> &str {
        &self.tz
    }

    pub fn led_color(&self) -> u32 {
        self.led_color
    }

    pub fn hours_24(&self) -> bool {
        self.hours_24
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Config {
    #[serde(rename = "wifiSsid")]
    pub wifi_ssid: String,
    #[serde(rename = "wifiPass")]
    pub wifi_pass: String,
    #[serde(rename = "timeZone")]
    pub time_zone: String,
    #[serde(rename = "ledColor")]
    pub led_color: String,
    pub hours_24: bool,
}

impl Config {
    pub fn new(
        wifi_ssid: &str,
        wifi_pass: &str,
        time_zone: &str,
        led_color: &str,
        hours_24: bool,
    ) -> Self {
        Config {
            wifi_ssid: String::from(wifi_ssid),
            wifi_pass: String::from(wifi_pass),
            time_zone: String::from(time_zone),
            led_color: String::from(led_color),
            hours_24,
        }
    }

    /// Basic validation without validator crate
    pub fn validate(&self) -> Result<(), &'static str> {
        // Check SSID is not empty
        if self.wifi_ssid.is_empty() {
            return Err("SSID must not be blank");
        }

        // Check timezone is not empty
        if self.time_zone.is_empty() {
            return Err("Time zone must not be blank");
        }

        // Check LED color format
        if self.led_color.len() != 7 {
            return Err("LED color must be 7 characters (#RRGGBB)");
        }

        if !self.led_color.starts_with('#') {
            return Err("LED color must start with #");
        }

        // Check hex digits
        for c in self.led_color.chars().skip(1) {
            if !c.is_ascii_hexdigit() {
                return Err("LED color must be valid hex");
            }
        }

        Ok(())
    }
}

impl From<InternalConfig> for Config {
    fn from(item: InternalConfig) -> Self {
        Config {
            wifi_ssid: item.wifi_ssid,
            wifi_pass: item.wifi_pass,
            time_zone: item.tz,
            led_color: format!("#{:06x}", item.led_color),
            hours_24: item.hours_24,
        }
    }
}

impl From<Config> for InternalConfig {
    fn from(item: Config) -> Self {
        // Parse hex color, defaulting to 0 if invalid
        let led_color = if item.led_color.starts_with('#') && item.led_color.len() == 7 {
            u32::from_str_radix(&item.led_color[1..], 16).unwrap_or(0)
        } else {
            0
        };

        InternalConfig {
            wifi_ssid: item.wifi_ssid,
            wifi_pass: item.wifi_pass,
            tz: item.time_zone,
            led_color,
            hours_24: item.hours_24,
        }
    }
}

pub trait Storage {
    fn set_raw(&mut self, name: &str, buf: &[u8]) -> Result<bool, &'static str>;
    fn get_raw<'a>(&self, name: &str, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, &'static str>;
}

pub struct InMemoryStorage {
    data: Option<Vec<u8>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        InMemoryStorage { data: None }
    }
}

impl Storage for InMemoryStorage {
    fn set_raw(&mut self, _name: &str, buf: &[u8]) -> Result<bool, &'static str> {
        self.data = Some(buf.to_vec());
        Ok(true)
    }

    fn get_raw<'a>(&self, _name: &str, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, &'static str> {
        match &self.data {
            Some(data) => {
                if buf.len() >= data.len() {
                    buf[..data.len()].copy_from_slice(data);
                    Ok(Some(&buf[..data.len()]))
                } else {
                    Err("Buffer too small")
                }
            }
            None => Ok(None),
        }
    }
}

pub struct ConfigStorage<S: Storage> {
    storage: S,
    config: Option<InternalConfig>,
}

impl<S: Storage> ConfigStorage<S> {
    pub fn new(storage: S) -> Self {
        ConfigStorage {
            storage,
            config: None,
        }
    }

    pub fn load(&mut self) -> Result<InternalConfig, &'static str> {
        if let Some(ref config) = self.config {
            return Ok(config.clone());
        }

        let mut buf = [0; 256];
        let config = match self.storage.get_raw("config", &mut buf) {
            Ok(Some(data)) => {
                match from_bytes::<InternalConfig>(data) {
                    Ok(config) => config,
                    Err(_) => InternalConfig::default(),
                }
            }
            _ => InternalConfig::default(),
        };
        
        self.config = Some(config.clone());
        Ok(config)
    }

    pub fn save(&mut self, config: &InternalConfig) -> Result<(), &'static str> {
        let serialized = to_vec::<InternalConfig, 256>(config)
            .map_err(|_| "Serialization failed")?;
        
        self.storage.set_raw("config", &serialized)?;
        self.config = Some(config.clone());
        Ok(())
    }
}