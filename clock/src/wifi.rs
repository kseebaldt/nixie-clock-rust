use anyhow::Result;

use drivers::config::{InternalConfig, DEFAULT_AP_PASS, DEFAULT_AP_SSID};
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi,
};

use log::info;

pub fn configure_wifi(
    wifi: &mut BlockingWifi<&mut EspWifi>,
    app_config: &InternalConfig,
) -> Result<()> {
    info!("Configuring wifi with SSID: {}", app_config.wifi_ssid());
    info!(
        "Configuring access point with SSID: {} Pass: {}",
        DEFAULT_AP_SSID, DEFAULT_AP_PASS
    );
    wifi.set_configuration(&Configuration::Mixed(
        ClientConfiguration {
            ssid: app_config.wifi_ssid().try_into().unwrap(),
            password: app_config.wifi_pass().try_into().unwrap(),
            auth_method: AuthMethod::None,
            ..Default::default()
        },
        AccessPointConfiguration {
            ssid: DEFAULT_AP_SSID.try_into().unwrap(),
            password: DEFAULT_AP_PASS.try_into().unwrap(),
            ..Default::default()
        },
    ))?;

    wifi.start()?;
    info!("Wifi started");

    wifi.connect()?;
    info!("Wifi connected");

    wifi.wait_netif_up()?;
    info!("Wifi netif up");

    Ok(())
}
