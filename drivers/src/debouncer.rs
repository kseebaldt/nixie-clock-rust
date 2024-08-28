//! Button Debouncer
//!
//! Uses integration algorithm from: <https://www.kennethkuhn.com/electronics/debounce.c>

use hal::digital::{InputPin, PinState};

pub struct Debouncer<T: InputPin> {
    integrator: u8,
    maximum: u8,
    state: PinState,
    pin: T,
}

impl<T: InputPin> Debouncer<T> {
    pub fn new(debounce_time: f32, sample_frequency: u32, pin: T) -> Self {
        Debouncer {
            integrator: 0,
            maximum: (debounce_time * sample_frequency as f32) as u8,
            state: PinState::Low,
            pin,
        }
    }

    pub fn update(&mut self) -> Result<(), T::Error> {
        if self.pin.is_low()? {
            if self.integrator > 0 {
                self.integrator -= 1;
            }
        } else if self.integrator < self.maximum {
            self.integrator += 1;
        }

        if self.integrator == 0 {
            self.state = PinState::Low;
        } else if self.integrator >= self.maximum {
            self.state = PinState::High;
        }
        Ok(())
    }
}

impl<T: InputPin> hal::digital::ErrorType for Debouncer<T> {
    type Error = T::Error;
}

impl<T: InputPin> InputPin for Debouncer<T> {
    fn is_high(&mut self) -> Result<bool, T::Error> {
        Ok(self.state == PinState::High)
    }

    fn is_low(&mut self) -> Result<bool, T::Error> {
        Ok(self.state == PinState::Low)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;

    #[derive(Debug)]
    struct FakeError {}

    struct FakePin {
        state: Rc<RefCell<PinState>>,
    }

    impl hal::digital::Error for FakeError {
        fn kind(&self) -> hal::digital::ErrorKind {
            hal::digital::ErrorKind::Other
        }
    }

    impl hal::digital::ErrorType for FakePin {
        type Error = FakeError;
    }

    impl InputPin for FakePin {
        fn is_high(&mut self) -> Result<bool, FakeError> {
            Ok(*self.state.borrow() == PinState::High)
        }

        fn is_low(&mut self) -> Result<bool, FakeError> {
            Ok(*self.state.borrow() == PinState::Low)
        }
    }

    #[test]
    fn it_debounces() {
        let input = "0100111011011001000011011010001001011100101111000100010111011100010";
        let integator = "0100123233233212100012123232101001012321212333210100010123233321010";
        let output = "0000001111111111100000001111100000000111111111110000000001111111000";

        let pin_state = Rc::new(RefCell::new(PinState::Low));
        let pin = FakePin {
            state: Rc::clone(&pin_state),
        };
        let mut debouncer = Debouncer::new(0.3, 10, pin);

        for (i, c) in input.chars().enumerate() {
            let new_state = match c.to_digit(10).unwrap() {
                0 => PinState::Low,
                1 => PinState::High,
                _ => unreachable!(),
            };
            let expected_state = match output.chars().nth(i).unwrap().to_digit(10).unwrap() {
                0 => false,
                1 => true,
                _ => unreachable!(),
            };

            pin_state.replace(new_state);
            debouncer.update().unwrap();

            assert!(
                debouncer.is_high().unwrap() == expected_state,
                "Failed at index {} - expected: {:?}, got: {:?} - integrator: {}",
                i,
                expected_state,
                debouncer.is_high().unwrap(),
                debouncer.integrator
            );
            assert!(
                debouncer.integrator
                    == integator.chars().nth(i).unwrap().to_digit(10).unwrap() as u8,
                "Failed at index {} - expected: {}, got: {}",
                i,
                integator.chars().nth(i).unwrap(),
                debouncer.integrator
            );
        }
    }
}
