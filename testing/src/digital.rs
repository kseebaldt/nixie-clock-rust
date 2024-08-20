use std::cell::RefCell;

use embedded_hal::digital::*;

pub struct FakePin<'a> {
    pub pin_number: u32,
    recorder: &'a Recorder,
}

pub struct Recorder {
    states: RefCell<Vec<(u32, PinState)>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            states: RefCell::new(vec![]),
        }
    }

    pub fn create_pin(&self, pin_number: u32) -> FakePin {
        FakePin::new(pin_number, self)
    }

    pub fn states(&self) -> Vec<(u32, PinState)> {
        return self.states.borrow().clone();
    }

    pub fn pin_states(&self, pin: &FakePin) -> Vec<PinState> {
        return self
            .states()
            .iter()
            .filter(|(p, _)| *p == pin.pin_number)
            .map(|(_, state)| state.clone())
            .collect();
    }    

    pub fn push_state(&self, pin: u32, state: PinState) {
        self.states.borrow_mut().push((pin, state));
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FakeError;

impl embedded_hal::digital::Error for FakeError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

impl<'a> ErrorType for FakePin<'a> {
    type Error = FakeError;
}

impl<'a> FakePin<'a> {
    pub fn new(pin_number: u32, recorder: &'a Recorder) -> Self {
        Self {
            pin_number,
            recorder,
        }
    }

    pub fn states(&self) -> Vec<PinState> {
        self.recorder.pin_states(self)
    }
}

impl<'a> OutputPin for FakePin<'a> {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.recorder
            .push_state(self.pin_number, PinState::High);
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.recorder
            .push_state(self.pin_number, PinState::Low);
        Ok(())
    }
}
