use crate::shift_register::Shift;
use chrono::{Datelike, Timelike};
use hal::digital::OutputPin;

pub enum DisplayMode {
    Time,
    Date,
    Year,
}

pub enum HourFormat {
    TwelveHour,
    TwentyFourHour,
}

pub struct NixieDisplay<'a, T, Pin1, Pin2> {
    shift_register: &'a mut T,
    seperator1: Pin1,
    seperator2: Pin2,
    mode: DisplayMode,
    hour_format: HourFormat,
}

impl<'a, T, Pin1, Pin2> NixieDisplay<'a, T, Pin1, Pin2>
where
    T: Shift,
    Pin1: OutputPin,
    Pin2: OutputPin,
{
    pub fn new(shift_register: &'a mut T, seperator1: Pin1, seperator2: Pin2) -> Self {
        Self {
            shift_register,
            seperator1,
            seperator2,
            mode: DisplayMode::Time,
            hour_format: HourFormat::TwelveHour,
        }
    }

    pub fn set_mode(&mut self, mode: DisplayMode) {
        self.mode = mode;
    }

    pub fn set_hour_format(&mut self, hour_format: HourFormat) {
        self.hour_format = hour_format;
    }

    pub fn next_mode(&mut self) {
        self.mode = match self.mode {
            DisplayMode::Time => DisplayMode::Date,
            DisplayMode::Date => DisplayMode::Year,
            DisplayMode::Year => DisplayMode::Time,
        }
    }

    pub fn display(&mut self, time: impl Timelike + Datelike) {
        match self.mode {
            DisplayMode::Time => self.display_time(time),
            DisplayMode::Date => self.display_date(time),
            DisplayMode::Year => self.display_year(time),
        }
    }

    pub fn display_time(&mut self, time: impl Timelike) {
        let hours = match self.hour_format {
            HourFormat::TwelveHour => time.hour12().1,
            HourFormat::TwentyFourHour => time.hour(),
        };
        let minutes = time.minute();
        let digits = [
            (hours / 10) as u8,
            (hours % 10) as u8,
            (minutes / 10) as u8,
            (minutes % 10) as u8,
        ];
        self.show_digits(&digits);
        if (time.second() % 2) == 0 {
            self.seperator1.set_high().unwrap();
            self.seperator2.set_high().unwrap();
        } else {
            self.seperator1.set_low().unwrap();
            self.seperator2.set_low().unwrap();
        }
    }

    pub fn display_date(&mut self, time: impl Datelike) {
        let month = time.month();
        let day = time.day();
        let digits = [
            (month / 10) as u8,
            (month % 10) as u8,
            (day / 10) as u8,
            (day % 10) as u8,
        ];
        self.show_digits(&digits);
        self.seperator1.set_low().unwrap();
        self.seperator2.set_high().unwrap();
    }

    pub fn display_year(&mut self, time: impl Datelike) {
        let year = time.year() as u16;
        let digits = [
            ((year / 1000) % 10) as u8,
            ((year / 100) % 10) as u8,
            ((year / 10) % 10) as u8,
            (year % 10) as u8,
        ];
        self.show_digits(&digits);
        self.seperator1.set_low().unwrap();
        self.seperator2.set_low().unwrap();
    }

    pub fn show_digits(&mut self, digits: &[u8]) {
        for i in (0..digits.len()).step_by(2) {
            let start = digits.len() - i - 1;
            let a = digits[start];
            let b = if digits.len() < i + 1 {
                0
            } else {
                digits[start - 1]
            };
            self.shift_register.shift(a * 16 + b);
        }
        self.shift_register.store();
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
    use hal::digital::PinState;
    use std::vec::Vec;
    use testing::digital::Recorder;

    use super::*;
    extern crate std;

    pub struct MockShift {
        current: Vec<u8>,
        values: Vec<Vec<u8>>,
    }

    impl MockShift {
        pub fn new() -> Self {
            Self {
                values: Vec::new(),
                current: Vec::new(),
            }
        }
    }

    impl Shift for MockShift {
        fn shift(&mut self, data: u8) {
            self.current.push(data);
        }

        fn store(&mut self) {
            self.values.push(self.current.clone());
            self.current = Vec::new();
        }
    }

    #[test]
    fn it_displays_time() {
        let mut mock = MockShift::new();
        let recorder = Recorder::new();
        let sep1 = recorder.create_pin(0);
        let sep2 = recorder.create_pin(1);

        let mut display = NixieDisplay::new(&mut mock, sep1, sep2);

        let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let time = NaiveTime::from_hms_opt(12, 34, 0).unwrap();
        let datetime = NaiveDateTime::new(date, time);
        display.display(datetime);

        let mut values = mock.values.into_iter();
        let value = values.next().unwrap();

        assert_eq!(value[0], 4 * 16 + 3);
        assert_eq!(value[1], 2 * 16 + 1);
    }

    #[test]
    fn it_turns_on_seperator_on_even_seconds() {
        let mut mock = MockShift::new();
        let recorder = Recorder::new();
        let mut sep1 = recorder.create_pin(0);
        let mut sep2 = recorder.create_pin(1);

        let mut display = NixieDisplay::new(&mut mock, &mut sep1, &mut sep2);

        let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let time = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        let datetime = NaiveDateTime::new(date, time);

        display.display(datetime);

        assert_eq!(sep1.states()[0], PinState::High);
        assert_eq!(sep2.states()[0], PinState::High);
    }

    #[test]
    fn it_turns_off_seperator_on_odd_seconds() {
        let mut mock = MockShift::new();
        let recorder = Recorder::new();
        let mut sep1 = recorder.create_pin(0);
        let mut sep2 = recorder.create_pin(1);

        let mut display = NixieDisplay::new(&mut mock, &mut sep1, &mut sep2);

        let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let time = NaiveTime::from_hms_opt(0, 0, 1).unwrap();
        let datetime = NaiveDateTime::new(date, time);

        display.display(datetime);

        assert_eq!(sep1.states()[0], PinState::Low);
        assert_eq!(sep2.states()[0], PinState::Low);
    }

    #[test]
    fn it_displays_date() {
        let mut mock = MockShift::new();
        let recorder = Recorder::new();
        let mut sep1 = recorder.create_pin(0);
        let mut sep2 = recorder.create_pin(1);

        let mut display = NixieDisplay::new(&mut mock, &mut sep1, &mut sep2);

        let date = NaiveDate::from_ymd_opt(2024, 12, 23).unwrap();
        let time = NaiveTime::from_hms_opt(12, 34, 0).unwrap();
        let datetime = NaiveDateTime::new(date, time);
        display.set_mode(DisplayMode::Date);
        display.display(datetime);

        let mut values = mock.values.into_iter();
        let value = values.next().unwrap();

        assert_eq!(value[0], 3 * 16 + 2);
        assert_eq!(value[1], 2 * 16 + 1);
        assert_eq!(sep1.states()[0], PinState::Low);
        assert_eq!(sep2.states()[0], PinState::High);
    }

    #[test]
    fn it_displays_year() {
        let mut mock = MockShift::new();
        let recorder = Recorder::new();
        let mut sep1 = recorder.create_pin(0);
        let mut sep2 = recorder.create_pin(1);

        let mut display = NixieDisplay::new(&mut mock, &mut sep1, &mut sep2);

        let date = NaiveDate::from_ymd_opt(2024, 12, 23).unwrap();
        let time = NaiveTime::from_hms_opt(12, 34, 0).unwrap();
        let datetime = NaiveDateTime::new(date, time);
        display.set_mode(DisplayMode::Year);
        display.display(datetime);

        let mut values = mock.values.into_iter();
        let value = values.next().unwrap();

        assert_eq!(value[0], 4 * 16 + 2);
        assert_eq!(value[1], 0 * 16 + 2);
        assert_eq!(sep1.states()[0], PinState::Low);
        assert_eq!(sep2.states()[0], PinState::Low);
    }
}
