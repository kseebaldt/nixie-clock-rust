# Migration Plan: esp-idf-svc to no_std esp-hal (UPDATED)

## Overview
This document outlines the migration path from esp-idf-svc (std) to esp-hal (no_std) while maintaining all existing functionality. **Updated with esp-wifi analysis showing better support than initially assessed.**

## Current Dependencies and Required Replacements

### 1. WiFi Networking
**Current**: `esp_idf_svc::wifi::{EspWifi, AsyncWifi, ClientConfiguration, AccessPointConfiguration}`
**Solution**: ✅ **esp-wifi provides full support**
- Mixed mode (AP+STA) is supported - see `wifi_access_point_with_sta.rs` example
- Embassy async support available - see `wifi_embassy_access_point_with_sta.rs`
- Uses `smoltcp` TCP/IP stack for networking
**Effort**: Medium - Different API but feature-complete

### 2. HTTP Server
**Current**: `esp_idf_svc::http::server::EspHttpServer`
**Solution**: ✅ **Use picoserve - perfect fit**
- Async no_std HTTP server framework
- Built-in routing, JSON support
- Designed for embassy/embedded use
- Handles all clock endpoints easily
**Effort**: Low - Direct port with cleaner API

### 3. Non-Volatile Storage (NVS)
**Current**: `esp_idf_svc::nvs::{EspNvs, EspNvsPartition}`
**Challenge**: No direct NVS equivalent - esp-storage only provides raw flash access
**Solutions**:
- Use `esp-storage` for low-level flash access (now in esp-hal)
- Add `sequential-storage` or `ekv` for key-value storage on top
- Or implement custom key-value layer (more work)
**Effort**: High - Need to integrate multiple crates or build custom solution

### 4. SNTP Time Synchronization
**Current**: `esp_idf_svc::sntp::EspSntp`
**Solution**: ✅ **Use `sntpc` crate**
- Supports no_std with async
- Works with smoltcp (used by esp-wifi)
- Embassy integration examples available
- Simple API for time retrieval
**Effort**: Low - Direct replacement with existing crate

### 5. GPIO and Hardware Control
**Current**: `esp_idf_svc::hal::gpio::*`
**Solution**: ✅ Direct replacement with `esp-hal` GPIO API
**Effort**: Low - APIs are similar

### 6. PWM/LEDC for RGB LED
**Current**: `esp_idf_svc::hal::ledc::*`
**Solution**: ✅ Use `esp-hal` LEDC peripheral
**Effort**: Low - Direct replacement available

### 7. Embassy Integration
**Current**: esp-idf-svc with embassy features
**Solution**: ✅ Use `esp-hal-embassy` 
**Effort**: Medium - Different initialization but similar usage

### 8. Memory Management
**Current**: std with heap allocation
**Challenge**: no_std requires static allocation or custom allocator
**Solutions**:
- Use `embedded-alloc` for heap-like allocation
- Refactor to use static buffers where possible
- Careful memory planning required

## Major Challenges (Updated)

1. **Storage**: NVS API doesn't exist, need custom flash-based solution
2. **Memory Constraints**: no_std requires careful memory management
3. **Missing std Features**: No `std::thread`, limited string handling, no dynamic vectors by default
4. **API Differences**: esp-wifi uses different APIs than esp-idf-svc
5. **Testing**: Need to verify all features work correctly in no_std

## Migration Steps

### Phase 1: Hardware Abstraction (Low Risk)
1. Replace GPIO usage with esp-hal
2. Migrate PWM/LEDC to esp-hal
3. Update shift register driver to use esp-hal traits

### Phase 2: Core Functionality (Medium Risk)
1. Set up no_std project structure with esp-hal
2. Add embedded-alloc for dynamic allocation
3. Port nixie display logic
4. Port configuration structures

### Phase 3: Networking (High Risk)
1. Integrate esp-wifi crate
2. Implement WiFi manager for mixed mode
3. Build minimal HTTP server
4. Port web API endpoints

### Phase 4: Storage (High Risk)
1. Implement flash-based key-value storage
2. Port configuration persistence
3. Add wear leveling if needed

### Phase 5: Time Services (Medium Risk)
1. Implement SNTP client
2. Integrate with embassy-time

## Estimated Effort (Final)

- **Hardware drivers**: 1-2 days
- **Core logic port**: 2-3 days  
- **WiFi integration**: 2-3 days (examples exist)
- **HTTP server**: 1 day (picoserve makes it easy)
- **Storage system**: 3-4 days (biggest challenge - need sequential-storage integration)
- **SNTP client**: 0.5-1 day (sntpc crate available)
- **Testing & debugging**: 3-5 days

**Total**: 10-12 days for full migration

## Alternatives to Consider

1. **esp-idf-hal**: A middle ground that provides no_std HAL but still uses ESP-IDF for WiFi/networking
2. **Partial Migration**: Keep esp-idf-svc for networking/storage, use esp-hal for hardware only
3. **Wait for Ecosystem**: esp-wifi and related crates are rapidly evolving

## Recommendation (Updated)

After analyzing esp-wifi examples, the migration is more feasible than initially assessed:

1. **Feasibility**: Migration IS possible - esp-wifi supports all needed WiFi features including mixed mode
2. **Main Challenge**: Storage system (NVS replacement) is the biggest hurdle
3. **Timeline**: 2-3 weeks of focused development

### Pros of Migration:
- Smaller binary size
- Better real-time performance
- More control over memory usage
- Active esp-hal development community

### Cons of Migration:
- Significant development effort
- Loss of mature ESP-IDF ecosystem
- Need to implement/integrate storage solution
- Risk of undiscovered limitations

### Final Recommendation:
The migration is technically feasible but represents a significant engineering effort. Consider migrating if:
- You need the performance/size benefits of no_std
- You're willing to invest 2-3 weeks in the migration
- You're comfortable with lower-level programming

Otherwise, esp-idf-svc remains a solid, production-ready choice.

## Key Resources for Migration

1. **esp-wifi examples**: https://github.com/esp-rs/esp-hal/tree/main/examples
2. **Embassy book**: https://embassy.dev/book/dev/index.html
3. **smoltcp docs**: https://docs.rs/smoltcp/latest/smoltcp/
4. **sequential-storage**: https://github.com/tweedegolf/sequential-storage
5. **picoserve**: https://github.com/sammhicks/picoserve