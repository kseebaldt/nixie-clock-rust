use embedded_hal::digital::OutputPin;
use esp_println::println;
use chrono::{DateTime, Datelike, Timelike};

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
    data_pin: &'a mut DATA,
    clock_pin: &'a mut CLK,
    latch_pin: &'a mut LATCH,
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
            data_pin: data,
            clock_pin: clock,
            latch_pin: latch,
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

    /// Simple shift register implementation inline
    fn shift_byte(&mut self, byte: u8) -> Result<(), CLK::Error> {
        for i in 0..8 {
            // Set data pin based on bit (MSB first)
            let bit = (byte >> (7 - i)) & 1;
            if bit == 1 {
                let _ = self.data_pin.set_high();
            } else {
                let _ = self.data_pin.set_low();
            }
            
            // Clock pulse
            let _ = self.clock_pin.set_high();
            let _ = self.clock_pin.set_low();
        }
        Ok(())
    }

    /// Latch the data to output
    fn latch(&mut self) -> Result<(), CLK::Error> {
        let _ = self.latch_pin.set_high();
        let _ = self.latch_pin.set_low();
        Ok(())
    }

    /// Display 4 digits on the nixie tubes
    pub fn display_digits(&mut self, digits: &[u8; 4]) -> Result<(), CLK::Error> {
        // Process digits in pairs from right to left (same as original)
        for i in (0..digits.len()).step_by(2) {
            let start = digits.len() - i - 1;
            let a = digits[start];
            let b = if digits.len() < i + 1 {
                0
            } else {
                digits[start - 1]
            };
            
            let byte_value = a * 16 + b;
            self.shift_byte(byte_value)?;
        }
        
        // Latch the data
        self.latch()?;
        Ok(())
    }

    /// Update display based on current mode and time
    pub fn update(&mut self, now: &DateTime<chrono::Utc>) -> Result<(), CLK::Error> {
        match self.current_mode {
            DisplayMode::Time => self.display_time(now),
            DisplayMode::Date => self.display_date(now),
            DisplayMode::Year => self.display_year(now),
        }
    }

    fn display_time(&mut self, now: &DateTime<chrono::Utc>) -> Result<(), CLK::Error> {
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

    fn display_date(&mut self, now: &DateTime<chrono::Utc>) -> Result<(), CLK::Error> {
        let month = now.month();
        let day = now.day();
        
        // Display MM:DD
        let m1 = (month / 10) as u8;
        let m2 = (month % 10) as u8;
        let d1 = (day / 10) as u8;
        let d2 = (day % 10) as u8;
        
        self.display_digits(&[m1, m2, d1, d2])
    }

    fn display_year(&mut self, now: &DateTime<chrono::Utc>) -> Result<(), CLK::Error> {
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