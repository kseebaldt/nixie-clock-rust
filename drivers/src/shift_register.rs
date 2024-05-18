use hal::digital::{OutputPin, PinState};

pub trait Shift {
    fn shift(&mut self, data: u8);
    fn store(&mut self);
}

pub struct ShiftRegister<'a, DataPin, ClockPin, LatchPin> {
    data: &'a mut DataPin,
    clock: &'a mut ClockPin,
    latch: &'a mut LatchPin,
}

impl<'a, DataPin, ClockPin, LatchPin> ShiftRegister<'a, DataPin, ClockPin, LatchPin>
where
    DataPin: OutputPin,
    ClockPin: OutputPin,
    LatchPin: OutputPin,
{
    pub fn new(data: &'a mut DataPin, clock: &'a mut ClockPin, latch: &'a mut LatchPin) -> Self {
        Self { data, clock, latch }
    }
}

impl<'a, DataPin, ClockPin, LatchPin> Shift for ShiftRegister<'a, DataPin, ClockPin, LatchPin>
where
    DataPin: OutputPin,
    ClockPin: OutputPin,
    LatchPin: OutputPin,
{
    fn shift(&mut self, data: u8) {
        for i in 0..8 {
            let state = if data & (1 << (7 - i)) == 0 {
                PinState::Low
            } else {
                PinState::High
            };
            self.data.set_state(state).unwrap();
            self.clock.set_high().unwrap();
            self.clock.set_low().unwrap();
        }
    }

    fn store(&mut self) {
        self.latch.set_high().unwrap();
        self.latch.set_low().unwrap();
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use testing::digital::FakePin;
    use std::vec;

    #[test]
    fn it_works() {
        let mut data_pin = FakePin::new();
        let mut clock_pin = FakePin::new();
        let mut latch_pin = FakePin::new();

        let mut r: ShiftRegister<FakePin, FakePin, FakePin> =
            ShiftRegister::new(&mut data_pin, &mut clock_pin, &mut latch_pin);

        r.shift(5);
        r.store();

        assert_eq!(
            data_pin.states(),
            vec![
                PinState::Low,
                PinState::Low,
                PinState::Low,
                PinState::Low,
                PinState::Low,
                PinState::High,
                PinState::Low,
                PinState::High
            ]
        );
        assert_eq!(
            clock_pin.states(),
            vec![
                PinState::High,
                PinState::Low,
                PinState::High,
                PinState::Low,
                PinState::High,
                PinState::Low,
                PinState::High,
                PinState::Low,
                PinState::High,
                PinState::Low,
                PinState::High,
                PinState::Low,
                PinState::High,
                PinState::Low,
                PinState::High,
                PinState::Low
            ]
        );
        assert_eq!(latch_pin.states(), vec![PinState::High, PinState::Low]);
    }
}
