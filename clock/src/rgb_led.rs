use esp_idf_hal::peripheral::Peripheral;
use esp_idf_svc::hal::{gpio::OutputPin, ledc::*, prelude::*, sys::EspError};

pub struct RgbLed<'a> {
    red: LedcDriver<'a>,
    green: LedcDriver<'a>,
    blue: LedcDriver<'a>,
}

impl<'d> RgbLed<'d> {
    pub fn create_driver<C, T>(
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

    pub fn new(
        red: LedcDriver<'d>,
        green: LedcDriver<'d>,
        blue: LedcDriver<'d>,
    ) -> Result<Self, EspError> {
        Ok(Self { red, green, blue })
    }

    pub fn set_rgb(&mut self, r: u8, g: u8, b: u8) -> Result<(), EspError> {
        self.red
            .set_duty(self.red.get_max_duty() * (r as u32) / 256)?;
        self.green
            .set_duty(self.green.get_max_duty() * (g as u32) / 256)?;
        self.blue
            .set_duty(self.blue.get_max_duty() * (b as u32) / 256)?;

        Ok(())
    }

    pub fn set_color(&mut self, color: u32) -> Result<(), EspError> {
        let r = (color >> 16) as u8;
        let g = (color >> 8) as u8;
        let b = color as u8;
        self.set_rgb(r, g, b)?;

        Ok(())
    }
}
