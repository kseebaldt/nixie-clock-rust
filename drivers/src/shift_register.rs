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
    use super::*;
    use embedded_hal_mock::eh1::pin::{
        Mock as PinMock, State as PinState, Transaction as PinTransaction,
    };

    #[test]
    fn it_works() {
        let mut data_pin = PinMock::new(&[
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
        ]);
        let mut clock_pin = PinMock::new(&[
            PinTransaction::set(PinState::High),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
            PinTransaction::set(PinState::Low),
        ]);
        let mut latch_pin = PinMock::new(&[
            PinTransaction::set(PinState::High),
            PinTransaction::set(PinState::Low),
        ]);

        let mut r: ShiftRegister<PinMock, PinMock, PinMock> =
            ShiftRegister::new(&mut data_pin, &mut clock_pin, &mut latch_pin);

        r.shift(5);
        r.store();

        data_pin.done();
        clock_pin.done();
        latch_pin.done();
    }
}
