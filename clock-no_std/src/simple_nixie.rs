use drivers::shift_register::{Shift, ShiftRegister};
use embedded_hal::digital::OutputPin;
use esp_println::println;

pub struct SimpleNixie<'a, CLK, LATCH, DATA>
where
    CLK: OutputPin,
    LATCH: OutputPin,
    DATA: OutputPin,
{
    shift_register: ShiftRegister<'a, DATA, CLK, LATCH>,
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
        }
    }

    /// Display 4 digits on the nixie tubes
    /// Uses the exact same logic as the original show_digits method
    pub fn display_digits(&mut self, digits: &[u8; 4]) -> Result<(), CLK::Error> {
        println!("display_digits called with: {:?}", digits);
        
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
            println!("Iteration {}: start={}, a={}, b={}, byte_value=0x{:02X}", i, start, a, b, byte_value);
            self.shift_register.shift(byte_value);
        }
        
        // Latch the data
        println!("Storing/latching data");
        self.shift_register.store();
        Ok(())
    }

    /// Test pattern - light up one digit on each tube
    pub fn test_pattern(&mut self) -> Result<(), CLK::Error> {
        self.display_digits(&[1, 2, 3, 4])
    }
}