use esp_idf_hal::peripheral::Peripheral;
use esp_idf_svc::hal::{gpio::OutputPin, ledc::*, prelude::*, sys::EspError};

pub struct RgbLed<'a> {
    red: LedcDriver<'a>,
    green: LedcDriver<'a>,
    blue: LedcDriver<'a>,
}

impl<'d> RgbLed<'d> {
    pub fn new<
        C1: LedcChannel,
        C2: LedcChannel,
        C3: LedcChannel,
        T1: LedcTimer,
        T2: LedcTimer,
        T3: LedcTimer
    >(
        red_channel: impl Peripheral<P = C1> + 'd,
        red_timer: impl Peripheral<P = T1> + 'd,
        red_pin: impl Peripheral<P = impl OutputPin> + 'd,
        green_channel: impl Peripheral<P = C2> + 'd,
        green_timer: impl Peripheral<P = T2> + 'd,
        green_pin: impl Peripheral<P = impl OutputPin> + 'd,
        blue_channel: impl Peripheral<P = C3> + 'd,
        blue_timer: impl Peripheral<P = T3> + 'd,
        blue_pin: impl Peripheral<P = impl OutputPin> + 'd,
    ) -> Result<Self, EspError> {
        let red = LedcDriver::new(
            red_channel,
            LedcTimerDriver::new(
                red_timer,
                &config::TimerConfig::new().frequency(5.kHz().into()),
            )?,
            red_pin,
        )?;
        let green = LedcDriver::new(
            green_channel,
            LedcTimerDriver::new(
                green_timer,
                &config::TimerConfig::new().frequency(5.kHz().into()),
            )?,
            green_pin,
        )?;
        let blue = LedcDriver::new(
            blue_channel,
            LedcTimerDriver::new(
                blue_timer,
                &config::TimerConfig::new().frequency(5.kHz().into()),
            )?,
            blue_pin,
        )?;
        Ok(Self { red, green, blue })
    }

    pub fn set_rgb(&mut self, r: u8, g: u8, b: u8) -> Result<(), EspError> {
        self.red.set_duty(self.red.get_max_duty() * (r as u32) / 256)?;
        self.green.set_duty(self.green.get_max_duty() * (g as u32) / 256)?;
        self.blue.set_duty(self.blue.get_max_duty() * (b as u32) / 256)?;

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
