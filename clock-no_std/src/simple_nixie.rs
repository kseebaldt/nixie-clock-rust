use drivers::shift_register::{Shift, ShiftRegister};
use embedded_hal::digital::OutputPin;
use esp_println::println;
use chrono::{DateTime, Datelike, Timelike, Utc};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayMode {
    Time,
    Date,
    Year,
}

impl DisplayMode {
    pub fn next(&self) -> Self {
        match self {
            DisplayMode::Time => DisplayMode::Date,
            DisplayMode::Date => DisplayMode::Year,
            DisplayMode::Year => DisplayMode::Time,
        }
    }
}

pub struct SimpleNixie<'a, CLK, LATCH, DATA>
where
    CLK: OutputPin,
    LATCH: OutputPin,
    DATA: OutputPin,
{
    shift_register: ShiftRegister<'a, DATA, CLK, LATCH>,
    current_mode: DisplayMode,
    hours_24: bool,
}

impl<'a, CLK, LATCH, DATA> SimpleNixie<'a, CLK, LATCH, DATA>
where
    CLK: OutputPin,
    LATCH: OutputPin,
    DATA: OutputPin,
{
    pub fn new(data: &'a mut DATA, clock: &'a mut CLK, latch: &'a mut LATCH) -> Self {
        Self {
            shift_register: ShiftRegister::new(data, clock, latch),
            current_mode: DisplayMode::Time,
            hours_24: false,
        }
    }

    pub fn set_mode(&mut self, mode: DisplayMode) {
        self.current_mode = mode;
        println!("Display mode changed to: {:?}", mode);
    }

    pub fn next_mode(&mut self) {
        self.current_mode = self.current_mode.next();
        println!("Display mode cycled to: {:?}", self.current_mode);
    }

    pub fn get_mode(&self) -> DisplayMode {
        self.current_mode
    }

    pub fn set_24_hour_format(&mut self, hours_24: bool) {
        self.hours_24 = hours_24;
    }

    /// Display 4 digits on the nixie tubes
    /// Uses the exact same logic as the original show_digits method
    pub fn display_digits(&mut self, digits: &[u8; 4]) -> Result<(), CLK::Error> {
        // Process digits exactly like the original show_digits method
        // This processes in pairs from right to left
        
        for i in (0..digits.len()).step_by(2) {
            let start = digits.len() - i - 1;
            let a = digits[start];
            let b = if digits.len() < i + 1 {
                0
            } else {
                digits[start - 1]
            };
            
            let byte_value = a * 16 + b;
            self.shift_register.shift(byte_value);
        }
        
        // Latch the data
        self.shift_register.store();
        Ok(())
    }

    /// Update display based on current mode and time
    pub fn update(&mut self, now: &DateTime<Utc>) -> Result<(), CLK::Error> {
        match self.current_mode {
            DisplayMode::Time => self.display_time(now),
            DisplayMode::Date => self.display_date(now),
            DisplayMode::Year => self.display_year(now),
        }
    }

    fn display_time(&mut self, now: &DateTime<Utc>) -> Result<(), CLK::Error> {
        let mut hour = now.hour();
        
        // Convert to 12-hour format if needed
        if !self.hours_24 {
            hour = match hour {
                0 => 12,
                13..=23 => hour - 12,
                _ => hour,
            };
        }
        
        let minute = now.minute();
        
        // Display HH:MM
        let h1 = (hour / 10) as u8;
        let h2 = (hour % 10) as u8;
        let m1 = (minute / 10) as u8;
        let m2 = (minute % 10) as u8;
        
        self.display_digits(&[h1, h2, m1, m2])
    }

    fn display_date(&mut self, now: &DateTime<Utc>) -> Result<(), CLK::Error> {
        let month = now.month();
        let day = now.day();
        
        // Display MM:DD
        let m1 = (month / 10) as u8;
        let m2 = (month % 10) as u8;
        let d1 = (day / 10) as u8;
        let d2 = (day % 10) as u8;
        
        self.display_digits(&[m1, m2, d1, d2])
    }

    fn display_year(&mut self, now: &DateTime<Utc>) -> Result<(), CLK::Error> {
        let year = now.year();
        
        // Display YYYY
        let y1 = ((year / 1000) % 10) as u8;
        let y2 = ((year / 100) % 10) as u8;
        let y3 = ((year / 10) % 10) as u8;
        let y4 = (year % 10) as u8;
        
        self.display_digits(&[y1, y2, y3, y4])
    }

    /// Test pattern - light up one digit on each tube
    pub fn test_pattern(&mut self) -> Result<(), CLK::Error> {
        self.display_digits(&[1, 2, 3, 4])
    }
}