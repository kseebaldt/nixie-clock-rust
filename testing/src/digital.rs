use embedded_hal::digital::*;

pub struct FakePin {
    states: Vec<PinState>,
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
        Self { states: vec![] }
    }

    pub fn states(&self) -> &[PinState] {
        &self.states
    }
}

impl OutputPin for FakePin {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.states.push(PinState::High);
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.states.push(PinState::Low);
        Ok(())
    }
}
