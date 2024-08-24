use postcard::{from_bytes, to_vec};
use serde::{Deserialize, Serialize};

use crate::storage::{Storage, StorageError};

#[toml_cfg::toml_config]
struct WifiConfig {
    #[default("Wokwi-GUEST")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_pass: &'static str,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Config {
    wifi_ssid: String,
    wifi_pass: String,
    tz: String,
    led_color: u32,
}

impl Default for Config {
    fn default() -> Self {
        let wifi_config = WIFI_CONFIG;
        Config::new(wifi_config.wifi_ssid, wifi_config.wifi_pass, "America/Chicago", 0x00000088)
    }
}

impl Config {
    pub fn new(wifi_ssid: &str, wifi_pass: &str, tz: &str, led_color: u32) -> Self {
        Config {
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

pub struct ConfigStorage {
    storage: Box<dyn Storage + Send>,
    config: Option<Config>
}

impl ConfigStorage {
    pub fn new(storage: Box<dyn Storage + Send>) -> Self {
        ConfigStorage { storage, config: None }
    }

    pub fn load(&mut self) -> Result<Config, ()> {
        if let Some(ref config) = self.config {
            return Ok(config.clone());
        }

        let mut buf = [0; 256];
        let config = match self.storage.get_raw("config", &mut buf) {
            Ok(Some(v)) => from_bytes::<Config>(v).unwrap(),
            _ => Config::default(),
        };
        self.config = Some(config.clone());

        Ok(config)
    }

    pub fn save(&mut self, config: &Config) -> Result<(), StorageError> {
        self.storage.set_raw(
            "config",
            &to_vec::<Config, 100>(&config).unwrap(),
        )?;

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

        assert_eq!(config, Config::default());
    }

    #[test]
    fn it_loads_previously_saved_config() {
        let storage = InMemoryStorage::new();
        let mut config_storage = ConfigStorage::new(Box::new(storage));

        let config = Config::new("ssid", "pass", "America/Chicago", 0x12345678);
        config_storage.save(&config).unwrap();

        assert_eq!(config, config_storage.load().unwrap());
    }
}