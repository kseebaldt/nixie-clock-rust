use esp_idf_hal::peripheral::Peripheral;
use esp_idf_svc::hal::{gpio::OutputPin, ledc::*, prelude::*, sys::EspError};


pub fn create_driver<'d, C, T>(
    channel: impl Peripheral<P = C> + 'd,
    timer: impl Peripheral<P = T> + 'd,
    pin: impl Peripheral<P = impl OutputPin> + 'd,
) -> Result<LedcDriver<'d>, EspError>
where
    C: LedcChannel<SpeedMode = <T as LedcTimer>::SpeedMode>,
    T: LedcTimer + 'd,
{
    let driver = LedcDriver::new(
        channel,
        LedcTimerDriver::new(timer, &config::TimerConfig::new().frequency(5.kHz().into()))?,
        pin,
    )?;
    Ok(driver)
}
