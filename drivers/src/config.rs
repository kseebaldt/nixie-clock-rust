use postcard::{from_bytes, to_vec};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::storage::{Storage, StorageError};

#[toml_cfg::toml_config]
struct DefaultConfig {
    #[default("Wokwi-GUEST")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_pass: &'static str,
    #[default("nixie-clock")]
    ap_ssid: &'static str,
    #[default("")]
    ap_pass: &'static str,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct InternalConfig {
    wifi_ssid: String,
    wifi_pass: String,
    tz: String,
    led_color: u32,
}

impl Default for InternalConfig {
    fn default() -> Self {
        let wifi_config = DEFAULT_CONFIG;
        InternalConfig::new(
            wifi_config.wifi_ssid,
            wifi_config.wifi_pass,
            "US/Central",
            0x00000088,
        )
    }
}

impl InternalConfig {
    pub fn new(wifi_ssid: &str, wifi_pass: &str, tz: &str, led_color: u32) -> Self {
        InternalConfig {
            wifi_ssid: String::from(wifi_ssid),
            wifi_pass: String::from(wifi_pass),
            tz: String::from(tz),
            led_color,
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
}

fn validate_color(color: &str) -> Result<(), ValidationError> {
    for (i, c) in color.chars().enumerate() {
        match (i, c) {
            (0, '#') => continue,
            (_, '0'..='9') | (_, 'a'..='f') | (_, 'A'..='F') => continue,
            _ => {
                return Err(ValidationError::new("invalid_color"));
            }
        }
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Validate)]
pub struct Config {
    #[validate(length(min = 1, message = "SSID must not be blank"))]
    #[serde(rename = "wifiSsid")]
    wifi_ssid: String,
    #[validate(length(min = 0))]
    #[serde(rename = "wifiPass")]
    wifi_pass: String,
    #[validate(length(min = 1, message = "time zone must not be blank"))]
    #[serde(rename = "timeZone")]
    time_zone: String,
    #[validate(
        length(equal = 7, message = "led color is invalid"),
        custom(function = "validate_color", message = "led color is invalid")
    )]
    #[serde(rename = "ledColor")]
    led_color: String,
}

impl Config {
    pub fn new(wifi_ssid: &str, wifi_pass: &str, time_zone: &str, led_color: &str) -> Self {
        Config {
            wifi_ssid: String::from(wifi_ssid),
            wifi_pass: String::from(wifi_pass),
            time_zone: String::from(time_zone),
            led_color: String::from(led_color),
        }
    }

    pub fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Validate::validate(self)
    }
}

impl From<InternalConfig> for Config {
    fn from(item: InternalConfig) -> Self {
        Config {
            wifi_ssid: item.wifi_ssid,
            wifi_pass: item.wifi_pass,
            time_zone: item.tz,
            led_color: format!("#{:06x}", item.led_color),
        }
    }
}

impl From<Config> for InternalConfig {
    fn from(item: Config) -> Self {
        InternalConfig {
            wifi_ssid: item.wifi_ssid,
            wifi_pass: item.wifi_pass,
            tz: item.time_zone,
            led_color: u32::from_str_radix(&item.led_color.replace("#", ""), 16).unwrap_or(0),
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
            Ok(Some(v)) => from_bytes::<InternalConfig>(v).unwrap(),
            _ => InternalConfig::default(),
        };
        self.config = Some(config.clone());

        Ok(config)
    }

    pub fn save(&mut self, config: &InternalConfig) -> Result<(), StorageError> {
        self.storage
            .set_raw("config", &to_vec::<InternalConfig, 100>(config).unwrap())?;

        self.config = Some(config.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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

        let config = InternalConfig::new("ssid", "pass", "US/Central", 0x123456);
        config_storage.save(&config).unwrap();

        assert_eq!(config, config_storage.load().unwrap());
    }

    #[test]
    fn it_updates_cache_on_save() {
        let storage = InMemoryStorage::new();
        let mut config_storage = ConfigStorage::new(Box::new(storage));

        let config = config_storage.load().unwrap();
        assert_eq!(config, InternalConfig::default());

        let config = InternalConfig::new("ssid", "pass", "US/Central", 0x123456);
        config_storage.save(&config).unwrap();

        assert_eq!(config, config_storage.load().unwrap());
    }

    #[test]
    fn convert_internal_config_to_config() {
        let config = InternalConfig::new("ssid", "pass", "US/Central", 0x123456);

        let expected = Config {
            wifi_ssid: "ssid".to_string(),
            wifi_pass: "pass".to_string(),
            time_zone: "US/Central".to_string(),
            led_color: "#123456".to_string(),
        };

        assert_eq!(expected, config.into());
    }

    #[test]
    fn convert_config_to_internal_config() {
        let expected = InternalConfig::new("ssid", "pass", "US/Central", 0x123456);

        let config = Config {
            wifi_ssid: "ssid".to_string(),
            wifi_pass: "pass".to_string(),
            time_zone: "US/Central".to_string(),
            led_color: "#123456".to_string(),
        };

        assert_eq!(expected, config.into());
    }

    #[test]
    fn valid_config() {
        let config: Config = Config::new("ssid", "pass", "US/Central", "#123456");

        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_wifi_ssid_is_not_blank() {
        let config: Config = Config::new(
            "", // Missing SSID
            "pass",
            "US/Central",
            "#123456",
        );

        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .field_errors()
            .get("wifi_ssid")
            .is_some());
    }

    #[test]
    fn validate_time_zone_is_not_blank() {
        let config: Config = Config::new(
            "ssid", "pass", "", // Missing time zone
            "#123456",
        );

        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .field_errors()
            .get("time_zone")
            .is_some());
    }

    #[test]
    fn validate_color_starts_with_hash() {
        let config: Config = Config::new(
            "ssid",
            "pass",
            "US/Central",
            "123456", // Missing #
        );

        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .field_errors()
            .get("led_color")
            .is_some());
    }

    #[test]
    fn validate_color_is_hex_color() {
        let config: Config = Config::new(
            "ssid",
            "pass",
            "US/Central",
            "#abcdeg", // Invalid hex color
        );

        let result = config.validate();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.field_errors().get("led_color").is_some());
    }
}
