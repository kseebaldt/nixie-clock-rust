# Storage Migration Example: NVS to sequential-storage

## Current NVS Usage (esp-idf-svc)

```rust
// Current code in your project
let nvs = EspNvs::new(partition, namespace, true)?;
nvs.set_blob("config", &serialized_config)?;
let config_data = nvs.get_blob("config", &mut buffer)?;
```

## Proposed no_std Solution with sequential-storage

### Dependencies
```toml
[dependencies]
esp-storage = "0.3"  # Raw flash access
sequential-storage = "1.0"  # Key-value storage
postcard = { version = "1.0", features = ["alloc"] }  # Serialization
```

### Implementation Example

```rust
use esp_storage::FlashStorage;
use sequential_storage::map::{store_item, fetch_item, MapError};

// Define a flash region for configuration storage
const CONFIG_FLASH_START: u32 = 0x3F0000;  // Example: Last 64KB of 4MB flash
const CONFIG_FLASH_SIZE: usize = 65536;

// Storage wrapper for your clock
pub struct ConfigStorage {
    flash: FlashStorage,
    flash_range: core::ops::Range<u32>,
}

impl ConfigStorage {
    pub fn new() -> Self {
        Self {
            flash: FlashStorage::new(),
            flash_range: CONFIG_FLASH_START..CONFIG_FLASH_START + CONFIG_FLASH_SIZE as u32,
        }
    }

    pub async fn save_config(&mut self, config: &Config) -> Result<(), StorageError> {
        // Serialize config
        let serialized = postcard::to_allocvec(config)
            .map_err(|_| StorageError::SerializationError)?;
        
        // Store using sequential-storage
        store_item(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut buffer,
            "config",  // Key
            &serialized,  // Value
        ).await
        .map_err(|e| match e {
            MapError::FullStorage => StorageError::StorageFull,
            _ => StorageError::WriteError,
        })
    }

    pub async fn load_config(&mut self) -> Result<Config, StorageError> {
        let mut buffer = [0u8; 1024];  // Max config size
        
        // Fetch from sequential-storage
        let (data, len) = fetch_item(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut buffer,
            "config",
        ).await
        .map_err(|_| StorageError::NotFound)?;
        
        // Deserialize
        postcard::from_bytes(&data[..len])
            .map_err(|_| StorageError::CorruptedData)
    }
}

// Error type
#[derive(Debug)]
pub enum StorageError {
    NotFound,
    StorageFull,
    WriteError,
    SerializationError,
    CorruptedData,
}
```

### Key Differences from NVS

1. **Manual Flash Management**: Need to allocate flash regions yourself
2. **Different API**: sequential-storage uses async/await
3. **Size Limits**: Must pre-allocate buffers for data
4. **No Namespaces**: Would need to implement namespace prefixes in keys
5. **Wear Leveling**: Built into sequential-storage (good!)

### Migration Steps

1. Replace `EspNvs` with `ConfigStorage` wrapper
2. Update all storage calls to use new async API
3. Define flash partition layout in code (not partition table)
4. Test thoroughly - different error handling

### Benefits
- Full control over storage implementation
- Power-fail safe by design
- Minimal flash wear with built-in leveling
- Works in no_std environment

### Drawbacks
- More complex than NVS's simple API
- Need to manage flash regions manually
- Less battle-tested than ESP-IDF's NVS