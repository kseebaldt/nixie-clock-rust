use alloc::boxed::Box;
use alloc::string::String;
use core::fmt::Write;

use postcard::{from_bytes, to_vec};
use serde::{Deserialize, Serialize};

use crate::storage::{Storage, StorageError};

/// Default WiFi SSID for station mode
pub const DEFAULT_WIFI_SSID: &str = "Wokwi-GUEST";
/// Default WiFi password for station mode
pub const DEFAULT_WIFI_PASS: &str = "";
/// Default SSID for access point mode
pub const DEFAULT_AP_SSID: &str = "nixie-clock";
/// Default password for access point mode
pub const DEFAULT_AP_PASS: &str = "";

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct InternalConfig {
    wifi_ssid: String,
    wifi_pass: String,
    tz: String,
    led_color: u32,
    hours_24: bool,
}

impl Default for InternalConfig {
    fn default() -> Self {
        InternalConfig::new(
            DEFAULT_WIFI_SSID,
            DEFAULT_WIFI_PASS,
            "US/Central",
            0x00000088,
            false,
        )
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

/// Validation error for Config
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub field: &'static str,
    pub message: &'static str,
}

impl ValidationError {
    pub fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Config {
    #[serde(rename = "wifiSsid")]
    wifi_ssid: String,
    #[serde(rename = "wifiPass")]
    wifi_pass: String,
    #[serde(rename = "timeZone")]
    time_zone: String,
    #[serde(rename = "ledColor")]
    led_color: String,
    hours_24: bool,
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

    pub fn validate(&self) -> Result<(), ValidationError> {
        // Validate SSID is not blank
        if self.wifi_ssid.is_empty() {
            return Err(ValidationError::new("wifi_ssid", "SSID must not be blank"));
        }

        // Validate time zone is not blank
        if self.time_zone.is_empty() {
            return Err(ValidationError::new(
                "time_zone",
                "time zone must not be blank",
            ));
        }

        // Validate LED color format: must be #RRGGBB
        if self.led_color.len() != 7 {
            return Err(ValidationError::new("led_color", "led color is invalid"));
        }

        if !self.led_color.starts_with('#') {
            return Err(ValidationError::new("led_color", "led color is invalid"));
        }

        // Validate hex characters
        for c in self.led_color[1..].chars() {
            if !c.is_ascii_hexdigit() {
                return Err(ValidationError::new(
                    "led_color",
                    "led color contains invalid hex characters",
                ));
            }
        }

        Ok(())
    }
}

/// Format a u32 color as #RRGGBB hex string
fn format_color(color: u32) -> String {
    let mut s = String::with_capacity(7);
    write!(s, "#{:06x}", color).unwrap();
    s
}

/// Parse a #RRGGBB hex string to u32
fn parse_color(color: &str) -> u32 {
    let hex = color.trim_start_matches('#');
    u32::from_str_radix(hex, 16).unwrap_or(0)
}

impl From<InternalConfig> for Config {
    fn from(item: InternalConfig) -> Self {
        Config {
            wifi_ssid: item.wifi_ssid,
            wifi_pass: item.wifi_pass,
            time_zone: item.tz,
            led_color: format_color(item.led_color),
            hours_24: item.hours_24,
        }
    }
}

impl From<Config> for InternalConfig {
    fn from(item: Config) -> Self {
        InternalConfig {
            wifi_ssid: item.wifi_ssid,
            wifi_pass: item.wifi_pass,
            tz: item.time_zone,
            led_color: parse_color(&item.led_color),
            hours_24: item.hours_24,
        }
    }
}

pub struct ConfigStorage {
    storage: Box<dyn Storage + Send>,
    config: Option<InternalConfig>,
}

impl ConfigStorage {
    pub fn new(storage: Box<dyn Storage + Send>) -> Self {
        ConfigStorage {
            storage,
            config: None,
        }
    }

    pub fn load(&mut self) -> Result<InternalConfig, StorageError> {
        if let Some(ref config) = self.config {
            return Ok(config.clone());
        }

        let mut buf = [0; 256];
        let config = match self.storage.get_raw("config", &mut buf) {
            Ok(Some(v)) => from_bytes::<InternalConfig>(v).unwrap_or_default(),
            _ => InternalConfig::default(),
        };
        self.config = Some(config.clone());

        Ok(config)
    }

    pub fn save(&mut self, config: &InternalConfig) -> Result<(), StorageError> {
        let data = to_vec::<InternalConfig, 100>(config).map_err(|_| StorageError::WriteError)?;
        self.storage.set_raw("config", &data)?;

        self.config = Some(config.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use crate::storage::InMemoryStorage;

    use super::*;

    #[test]
    fn it_returns_default_if_no_value_stored() {
        let storage = InMemoryStorage::new();
        let mut config_storage = ConfigStorage::new(Box::new(storage));

        let config = config_storage.load().unwrap();

        assert_eq!(config, InternalConfig::default());
    }

    #[test]
    fn it_loads_previously_saved_config() {
        let storage = InMemoryStorage::new();
        let mut config_storage = ConfigStorage::new(Box::new(storage));

        let config = InternalConfig::new("ssid", "pass", "US/Central", 0x123456, false);
        config_storage.save(&config).unwrap();

        assert_eq!(config, config_storage.load().unwrap());
    }

    #[test]
    fn it_updates_cache_on_save() {
        let storage = InMemoryStorage::new();
        let mut config_storage = ConfigStorage::new(Box::new(storage));

        let config = config_storage.load().unwrap();
        assert_eq!(config, InternalConfig::default());

        let config = InternalConfig::new("ssid", "pass", "US/Central", 0x123456, false);
        config_storage.save(&config).unwrap();

        assert_eq!(config, config_storage.load().unwrap());
    }

    #[test]
    fn it_converts_internal_config_to_config() {
        let config = InternalConfig::new("ssid", "pass", "US/Central", 0x123456, false);

        let expected = Config {
            wifi_ssid: "ssid".to_string(),
            wifi_pass: "pass".to_string(),
            time_zone: "US/Central".to_string(),
            led_color: "#123456".to_string(),
            hours_24: false,
        };

        assert_eq!(expected, config.into());
    }

    #[test]
    fn it_converts_config_to_internal_config() {
        let expected = InternalConfig::new("ssid", "pass", "US/Central", 0x123456, false);

        let config = Config {
            wifi_ssid: "ssid".to_string(),
            wifi_pass: "pass".to_string(),
            time_zone: "US/Central".to_string(),
            led_color: "#123456".to_string(),
            hours_24: false,
        };

        assert_eq!(expected, config.into());
    }

    #[test]
    fn it_validates_valid_config() {
        let config: Config = Config::new("ssid", "pass", "US/Central", "#123456", false);

        assert!(config.validate().is_ok());
    }

    #[test]
    fn it_validates_wifi_ssid_is_not_blank() {
        let config: Config = Config::new(
            "", // Missing SSID
            "pass",
            "US/Central",
            "#123456",
            false,
        );

        let result = config.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "wifi_ssid");
    }

    #[test]
    fn it_validates_time_zone_is_not_blank() {
        let config: Config = Config::new(
            "ssid", "pass", "", // Missing time zone
            "#123456", false,
        );

        let result = config.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "time_zone");
    }

    #[test]
    fn it_validates_color_starts_with_hash() {
        let config: Config = Config::new(
            "ssid",
            "pass",
            "US/Central",
            "123456", // Missing #
            false,
        );

        let result = config.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "led_color");
    }

    #[test]
    fn it_validates_color_is_hex_color() {
        let config: Config = Config::new(
            "ssid",
            "pass",
            "US/Central",
            "#abcdeg", // Invalid hex color
            false,
        );

        let result = config.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "led_color");
    }
}
