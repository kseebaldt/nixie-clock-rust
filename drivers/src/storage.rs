use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("error reading from storage")]
    ReadError,
    #[error("error writing to storage")]
    WriteError,
}

pub trait Storage {
    fn set_raw(&mut self, name: &str, buf: &[u8]) -> Result<bool, StorageError>;
    fn get_raw<'a>(&self, name: &str, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, StorageError>;
}

pub struct InMemoryStorage {
    storage: HashMap<String, Vec<u8>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        InMemoryStorage {
            storage: HashMap::new(),
        }
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl Storage for InMemoryStorage {
    fn set_raw(&mut self, name: &str, buf: &[u8]) -> Result<bool, StorageError> {
        self.storage.insert(name.to_string(), buf.to_vec());
        Ok(true)
    }

    fn get_raw<'a>(&self, name: &str, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, StorageError> {
        match self.storage.get(name) {
            Some(v) => {
                match buf.len().cmp(&v.len()) {
                    std::cmp::Ordering::Equal => {
                        buf.copy_from_slice(v);
                    }
                    std::cmp::Ordering::Greater => {
                        buf[..v.len()].copy_from_slice(v);
                    }
                    std::cmp::Ordering::Less => {
                        buf.copy_from_slice(&v[..buf.len()]);
                    }
                }
                Ok(Some(buf))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcard::{from_bytes, to_vec};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestStruct<'a> {
        some_bytes: &'a [u8],
        a_str: &'a str,
        a_number: i16,
    }

    #[test]
    fn test_in_memory_storage() {
        let mut storage = InMemoryStorage::new();

        let value: [u8; 3] = [1, 2, 3];

        let mut buf: [u8; 64] = [0; 64];

        storage.set_raw("test", &value).unwrap();

        assert_eq!(
            storage.get_raw("test", &mut buf).unwrap().unwrap()[..3],
            value
        );
    }

    #[test]
    fn test_serde() {
        let mut storage = InMemoryStorage::new();

        let my_struct = TestStruct {
            some_bytes: &[1, 2, 3, 4],
            a_str: "I'm a str inside a struct!",
            a_number: 42,
        };

        storage
            .set_raw("test", &to_vec::<TestStruct, 100>(&my_struct).unwrap())
            .unwrap();

        let buf: &mut [u8] = &mut [0; 100];

        let raw = storage.get_raw("test", buf).unwrap().unwrap();
        let value = from_bytes::<TestStruct>(raw).unwrap();

        assert_eq!(my_struct, value);
    }
}
