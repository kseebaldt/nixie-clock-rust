//! Flash storage implementation for ESP32 using sequential-storage
//!
//! This module provides wear-leveled, power-fail safe configuration storage
//! using the sequential-storage crate's map API.

use core::ops::Range;
use embassy_embedded_hal::adapter::BlockingAsync;
use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use esp_println::println;
use esp_storage::FlashStorage;
use sequential_storage::cache::NoCache;
use sequential_storage::map::{MapConfig, MapStorage};

/// Flash sector size for ESP32 (4KB)
pub const FLASH_SECTOR_SIZE: u32 = 4096;

/// Start address for config storage partition (from partitions.csv)
/// The "config" partition is at offset 0xe000 with size 0x2000 (8KB = 2 pages)
pub const CONFIG_FLASH_START: u32 = 0xe000;

/// End address for config storage partition
pub const CONFIG_FLASH_END: u32 = 0x10000; // 0xe000 + 0x2000

/// Flash range for sequential-storage
pub const CONFIG_FLASH_RANGE: Range<u32> = CONFIG_FLASH_START..CONFIG_FLASH_END;

/// Key used for storing the config in the map
pub const CONFIG_KEY: u8 = 0;

/// Wrapper around esp_storage::FlashStorage that implements the required traits
pub struct EspFlashStorage<'d> {
    flash: FlashStorage<'d>,
}

impl<'d> EspFlashStorage<'d> {
    /// Create a new flash storage wrapper
    pub fn new(flash: esp_hal::peripherals::FLASH<'d>) -> Self {
        Self {
            flash: FlashStorage::new(flash),
        }
    }
}

// Error type implementation
impl ErrorType for EspFlashStorage<'_> {
    type Error = <FlashStorage<'static> as ErrorType>::Error;
}

// Synchronous ReadNorFlash implementation
// With bytewise-read feature, READ_SIZE is 1
impl ReadNorFlash for EspFlashStorage<'_> {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.flash.read(offset, bytes)
    }

    fn capacity(&self) -> usize {
        self.flash.capacity()
    }
}

// Synchronous NorFlash implementation
impl NorFlash for EspFlashStorage<'_> {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = FLASH_SECTOR_SIZE as usize;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.flash.erase(from, to)
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.flash.write(offset, bytes)
    }
}

// SAFETY: EspFlashStorage is safe to send between threads when wrapped in a Mutex
unsafe impl Send for EspFlashStorage<'_> {}

/// High-level config storage API using sequential-storage
pub struct SeqConfigStorage<'d> {
    storage: MapStorage<u8, BlockingAsync<EspFlashStorage<'d>>, NoCache>,
}

impl<'d> SeqConfigStorage<'d> {
    /// Create a new config storage instance
    pub fn new(flash: esp_hal::peripherals::FLASH<'d>) -> Self {
        let esp_flash = EspFlashStorage::new(flash);
        // Use BlockingAsync adapter from embassy-embedded-hal
        let async_flash = BlockingAsync::new(esp_flash);
        let config = MapConfig::new(CONFIG_FLASH_RANGE);
        let cache = NoCache::new();

        Self {
            storage: MapStorage::new(async_flash, config, cache),
        }
    }

    /// Load config data from flash
    /// Returns None if no config is stored or if corrupted
    /// The data is written into `buf` and the length is returned
    pub async fn load(&mut self, buf: &mut [u8]) -> Option<usize> {
        // Use a work buffer for fetch_item
        let mut work_buf = [0u8; 512];

        // Fetch the config item as raw bytes
        match self
            .storage
            .fetch_item::<&[u8]>(&mut work_buf, &CONFIG_KEY)
            .await
        {
            Ok(Some(data)) => {
                let len = data.len();
                if len > buf.len() {
                    println!("SeqStorage: Buffer too small ({} < {})", buf.len(), len);
                    return None;
                }
                // Copy data to caller's buffer
                buf[..len].copy_from_slice(data);
                println!("SeqStorage: Loaded {} bytes from flash", len);
                Some(len)
            }
            Ok(None) => {
                println!("SeqStorage: No config found in flash");
                None
            }
            Err(e) => {
                println!("SeqStorage: Error loading config: {:?}", e);
                None
            }
        }
    }

    /// Save config data to flash
    pub async fn save(
        &mut self,
        data: &[u8],
    ) -> Result<(), sequential_storage::Error<esp_storage::FlashStorageError>> {
        println!("SeqStorage: Saving {} bytes to flash", data.len());

        // Work buffer for store_item
        let mut work_buf = [0u8; 512];

        // Store the config
        self.storage
            .store_item(&mut work_buf, &CONFIG_KEY, &data)
            .await?;

        println!("SeqStorage: Save complete");
        Ok(())
    }

    /// Erase all data in the config storage area
    pub async fn erase_all(
        &mut self,
    ) -> Result<(), sequential_storage::Error<esp_storage::FlashStorageError>> {
        println!("SeqStorage: Erasing all config data");
        self.storage.erase_all().await?;
        println!("SeqStorage: Erase complete");
        Ok(())
    }
}
