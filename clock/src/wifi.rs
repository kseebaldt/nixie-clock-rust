use anyhow::Result;

use drivers::config::{DefaultConfig, InternalConfig};
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi,
};

use log::info;

pub fn configure_wifi(
    wifi: &mut BlockingWifi<&mut EspWifi>,
    app_config: &InternalConfig,
    default_config: &DefaultConfig,
) -> Result<()> {
    info!("Configuring wifi with SSID: {}", app_config.wifi_ssid());
    info!(
        "Configuring access point with SSID: {} Pass: {}",
        default_config.ap_ssid, default_config.ap_pass
    );
    wifi.set_configuration(&Configuration::Mixed(
        ClientConfiguration {
            ssid: app_config.wifi_ssid().try_into().unwrap(),
            password: app_config.wifi_pass().try_into().unwrap(),
            auth_method: AuthMethod::None,
            ..Default::default()
        },
        AccessPointConfiguration {
            ssid: default_config.ap_ssid.try_into().unwrap(),
            password: default_config.ap_pass.try_into().unwrap(),
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
