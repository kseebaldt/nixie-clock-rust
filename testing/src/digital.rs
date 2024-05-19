use std::{cell::RefCell, sync::Arc};

use embedded_hal::digital::*;

pub struct FakePin {
    pin_number: u32,
    states: Arc<RefCell<Vec<(u32, PinState)>>>,
}

pub struct Recorder {
    states: Arc<RefCell<Vec<(u32, PinState)>>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            states: Arc::new(RefCell::new(vec![])),
        }
    }

    pub fn create_pin(&self, pin_number: u32) -> FakePin {
        FakePin {
            pin_number,
            states: self.states.clone(),
        }
    }

    pub fn states(&self) -> Vec<(u32, PinState)> {
        return self.states.borrow().clone();
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FakeError;

impl embedded_hal::digital::Error for FakeError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

impl ErrorType for FakePin {
    type Error = FakeError;
}

impl FakePin {
    pub fn new() -> Self {
        Self {
            pin_number: 0,
            states: Arc::new(RefCell::new(vec![])),
        }
    }

    pub fn states(&self) -> Vec<PinState> {
        return self
            .states
            .borrow()
            .iter()
            .filter(|(p, _)| *p == self.pin_number)
            .map(|(_, state)| state.clone())
            .collect();
    }
}

impl OutputPin for FakePin {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.states
            .borrow_mut()
            .push((self.pin_number, PinState::High));
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.states
            .borrow_mut()
            .push((self.pin_number, PinState::Low));
        Ok(())
    }
}
