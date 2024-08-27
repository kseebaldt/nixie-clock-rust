use hal::pwm::SetDutyCycle;

pub struct RgbLed<T: SetDutyCycle> {
    red: T,
    green: T,
    blue: T,
}

impl<T: SetDutyCycle> RgbLed<T> {
    pub fn new(red: T, green: T, blue: T) -> Self {
        RgbLed { red, green, blue }
    }

    pub fn set_rgb(&mut self, r: u8, g: u8, b: u8) -> Result<(), T::Error> {
        self.red
            .set_duty_cycle_fraction(256 - r as u16, 255)?;
        self.green
            .set_duty_cycle_fraction(256 - g as u16, 255)?;
        self.blue
            .set_duty_cycle_fraction(256 - b as u16, 255)?;

        Ok(())
    }

    pub fn set_color(&mut self, color: u32) -> Result<(), T::Error> {
        let r = (color >> 16) as u8;
        let g = (color >> 8) as u8;
        let b = color as u8;
        self.set_rgb(r, g, b)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::pwm::{ErrorKind, ErrorType};

    use super::*;

    #[derive(Debug)]
    struct FakeError {}
    
    impl hal::pwm::Error for FakeError {
        fn kind(&self) -> hal::pwm::ErrorKind {
            ErrorKind::Other 
        }
    }

    struct MockLed {
        duty: u16,
    }

    impl ErrorType for MockLed {
        type Error = FakeError;
    }
    
    impl SetDutyCycle for MockLed {
        fn max_duty_cycle(&self) -> u16 {
            1000
        }
    
        fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
            self.duty = duty;
            Ok(())
        }
    }

    #[test]
    fn it_sets_color() {
        let red = MockLed { duty: 0 };
        let green = MockLed { duty: 0 };
        let blue = MockLed { duty: 0 };

        let mut led = RgbLed::new(
            red,
            green,
            blue
        );

        let color = 0xFF8040;

        led.set_color(color).unwrap();

        assert_eq!(led.red.duty, 1000);
        assert_eq!(led.green.duty, 501);
        assert_eq!(led.blue.duty, 250);
    }

    #[test]
    fn it_sets_color2() {
        let red = MockLed { duty: 0 };
        let green = MockLed { duty: 0 };
        let blue = MockLed { duty: 0 };

        let mut led = RgbLed::new(
            red,
            green,
            blue
        );

        let color = 0xFFFFFF;

        led.set_color(color).unwrap();

        assert_eq!(led.red.duty, 1000);
        assert_eq!(led.green.duty, 1000);
        assert_eq!(led.blue.duty, 1000);
    }

    #[test]
    fn it_sets_color3() {
        let red = MockLed { duty: 0 };
        let green = MockLed { duty: 0 };
        let blue = MockLed { duty: 0 };

        let mut led = RgbLed::new(
            red,
            green,
            blue
        );

        let color = 0x000000;

        led.set_color(color).unwrap();

        assert_eq!(led.red.duty, 0);
        assert_eq!(led.green.duty, 0);
        assert_eq!(led.blue.duty, 0);
    }
}
