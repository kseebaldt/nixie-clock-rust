# ESP-Alloc Setup Example

## Basic Setup in main.rs

```rust
#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

extern crate alloc;
use esp_alloc as _;  // The allocator is initialized by the esp_alloc::heap! macro

// Initialize heap with 32KB
esp_alloc::heap!(32 * 1024);

#[esp_hal::entry]
fn main() -> ! {
    // Your main code here
    // The heap is automatically initialized
    
    // You can now use alloc types
    use alloc::vec::Vec;
    let mut v = Vec::new();
    v.push(42);
    
    // Continue with Embassy setup...
}
```

## With Heap Statistics (Optional)

```rust
// Enable heap statistics
esp_alloc::heap!(32 * 1024, stats: true);

#[esp_hal::entry]
fn main() -> ! {
    // Later in your code, you can print heap stats
    esp_alloc::print_stats();
    
    // Or get them programmatically
    let stats = esp_alloc::get_stats();
    println!("Free: {}, Used: {}", stats.free, stats.used);
}
```

## With PSRAM (If Available)

```rust
// If your ESP32 has PSRAM, you can use it
esp_alloc::psram_heap!(4 * 1024 * 1024);  // 4MB PSRAM heap

// Or split between internal and external
esp_alloc::heap!(32 * 1024);              // Internal RAM
esp_alloc::psram_heap!(4 * 1024 * 1024);  // External PSRAM
```

## Key Advantages for Nixie Clock

1. **Simple one-line setup**: Just add the macro
2. **No manual memory region configuration**: Unlike embedded-alloc
3. **Built-in debugging**: Stats help track memory usage
4. **ESP32 optimized**: Better performance on your target platform
5. **Embassy compatible**: Works great with async tasks

## Memory Size Recommendation

For the nixie clock, 32KB heap should be plenty:
- Web server buffers: ~8KB
- Configuration storage: ~1KB  
- String formatting: ~2KB
- Async task overhead: ~8KB
- Safety margin: ~13KB

Total: 32KB provides comfortable headroom