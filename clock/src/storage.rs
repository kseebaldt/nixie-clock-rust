use drivers::storage::{Storage, StorageError};
use esp_idf_svc::nvs::*;
use esp_idf_svc::sys::EspError;

pub struct NvsStorage<T: NvsPartitionId> {
    nvs: EspNvs<T>,
}

impl<T: NvsPartitionId> NvsStorage<T> {
    pub fn new(partition: EspNvsPartition<T>, namespace: &str) -> Result<Self, EspError> {
        let nvs = EspNvs::new(partition, namespace, true)?;
        Ok(NvsStorage { nvs })
    }
}

impl<T: NvsPartitionId> Storage for NvsStorage<T> {
    fn set_raw(&mut self, name: &str, buf: &[u8]) -> Result<bool, StorageError> {
        self.nvs
            .set_raw(name, buf)
            .map_err(|_| StorageError::WriteError)
    }

    fn get_raw<'a>(&self, name: &str, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, StorageError> {
        self.nvs
            .get_raw(name, buf)
            .map_err(|_| StorageError::ReadError)
    }
}
