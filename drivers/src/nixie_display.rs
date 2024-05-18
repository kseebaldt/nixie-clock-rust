use crate::shift_register::Shift;

pub struct NixieDisplay<'a, T> {
    shift_register: &'a mut T,
}

impl<'a, T: Shift> NixieDisplay<'a, T> {
    pub fn new(shift_register: &'a mut T) -> Self {
        Self { shift_register }
    }

    pub fn show(&mut self, digits: &[u8]) {
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
    use std::vec::Vec;

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
    fn it_writes() {
        let mut mock = MockShift::new();
        let mut display = NixieDisplay::new(&mut mock);

        display.show(&[1, 2, 3, 4]);
        display.show(&[5, 6, 7, 8]);
        let mut values = mock.values.into_iter();
        let value = values.next().unwrap();

        assert_eq!(value[0], 4 * 16 + 3);
        assert_eq!(value[1], 2 * 16 + 1);

        let value2 = values.into_iter().next().unwrap();

        assert_eq!(value2[0], 8 * 16 + 7);
        assert_eq!(value2[1], 6 * 16 + 5);

    }
}
