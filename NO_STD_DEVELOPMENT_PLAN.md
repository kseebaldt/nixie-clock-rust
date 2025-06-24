# No-Std Nixie Clock Development Plan

## Overview
This plan outlines building a no_std version of the nixie clock from scratch in `clock_no_std/` while maintaining all existing features. The development follows an incremental approach with testable milestones.

## Project Structure
```
nixie-clock-rust/
├── clock/              # Current esp-idf-svc version
├── clock_no_std/       # New no_std version
├── drivers/            # Shared hardware drivers (already no_std compatible)
├── webapp/             # Shared web interface
└── testing/            # Shared test infrastructure
```

## Development Phases

### Phase 1: Project Setup and Basic Hardware (Days 1-2)
**Goal**: Establish no_std project structure and verify basic hardware control

#### Tasks:
1. **Create clock_no_std project**
   ```bash
   cargo new clock_no_std --bin
   cd clock_no_std
   ```

2. **Configure Cargo.toml**
   ```toml
   [package]
   name = "clock_no_std"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   esp-hal = { version = "0.16", features = ["esp32", "embassy", "embassy-time-timg0"] }
   esp-backtrace = { version = "0.11", features = ["esp32", "panic-handler", "exception-handler", "print-uart"] }
   embassy-executor = { version = "0.5", features = ["executor-thread", "integrated-timers"] }
   embassy-time = { version = "0.3" }
   static_cell = "2"
   embedded-hal = "1.0"
   embedded-hal-async = "1.0"
   drivers = { path = "../drivers" }
   
   # Allocation
   esp-alloc = { version = "0.3", features = ["nightly"] }
   
   [profile.release]
   opt-level = "s"
   ```

3. **Basic main.rs with Embassy**
   - Set up esp-alloc (required for esp-wifi)
   - Initialize Embassy runtime
   - Basic GPIO test with LED
   - Use esp-generate with: alloc, wifi, embassy options

4. **Verify nixie display driver**
   - Port shift register control
   - Test displaying static numbers
   - Verify separator LED control

#### Milestone 1: Display "1234" on nixie tubes with blinking LED

---

### Phase 2: Display Logic and Time Management (Days 3-4)
**Goal**: Implement display modes and time tracking

#### Tasks:
1. **Port NixieDisplay module**
   - Time, date, year display modes
   - Separator blinking logic
   - 12/24 hour format support

2. **Implement button handling**
   - Software debouncing
   - Mode cycling (Time → Date → Year)
   - Embassy task for button monitoring

3. **Basic time tracking**
   - Use embassy-time for initial timekeeping
   - Implement display update loop
   - Test mode transitions

#### Milestone 2: Working display with button-controlled mode switching (using fake time)

---

### Phase 3: Configuration System (Days 5-6)
**Goal**: Implement persistent configuration storage

#### Tasks:
1. **Add storage dependencies**
   ```toml
   esp-storage = "0.3"
   sequential-storage = "1.0"
   postcard = { version = "1.0", default-features = false }
   serde = { version = "1.0", default-features = false, features = ["derive"] }
   ```

2. **Define configuration flash region**
   - Allocate last 64KB for config storage
   - Initialize sequential-storage

3. **Implement ConfigStorage**
   - Save/load configuration
   - Default configuration
   - Migration from Config to InternalConfig

4. **RGB LED control**
   - LEDC PWM setup
   - Color configuration support
   - Common anode inversion

#### Milestone 3: Persistent configuration with RGB LED showing configured color

---

### Phase 4: WiFi and Networking (Days 7-8)
**Goal**: Establish WiFi connectivity with mixed mode

#### Tasks:
1. **Add WiFi dependencies**
   ```toml
   esp-wifi = { version = "0.4", features = ["esp32", "embassy-net", "wifi", "async"] }
   embassy-net = { version = "0.4", features = ["tcp", "udp", "dhcpv4", "medium-ethernet"] }
   smoltcp = { version = "0.11", default-features = false, features = ["proto-dhcpv4", "socket-tcp", "socket-udp"] }
   heapless = "0.8"
   ```

2. **Initialize esp-wifi**
   - Set up WiFi peripheral
   - Configure embassy-net stack

3. **Implement mixed mode**
   - Station mode for internet
   - Access point for configuration
   - Port IP address configuration

4. **WiFi manager task**
   - Connection monitoring
   - Automatic reconnection
   - Hot-reload configuration

#### Milestone 4: ESP32 connects to WiFi and creates access point simultaneously

---

### Phase 5: Time Synchronization (Day 9)
**Goal**: Implement SNTP client for accurate time

#### Tasks:
1. **Add SNTP dependency**
   ```toml
   sntpc = { version = "0.3", default-features = false, features = ["async"] }
   chrono = { version = "0.4", default-features = false }
   chrono-tz = { version = "0.8", default-features = false }
   ```

2. **Implement SNTP task**
   - Create UDP socket
   - Query NTP servers
   - Update system time

3. **Timezone support**
   - Parse timezone from config
   - Apply timezone offset
   - Handle DST transitions

#### Milestone 5: Clock displays accurate network time in configured timezone

---

### Phase 6: Web Server (Days 10-11)
**Goal**: Implement web configuration interface

#### Tasks:
1. **Add web server dependencies**
   ```toml
   picoserve = "0.10"
   serde_json = { version = "1.0", default-features = false }
   ```

2. **Embed web interface**
   - Include webapp/dist/index.html
   - Set up static file serving

3. **Implement REST API**
   - GET /config endpoint
   - POST /config with validation
   - Error handling

4. **Configuration hot-reload**
   - Channel between web server and main
   - Apply changes immediately

#### Milestone 6: Full web configuration interface accessible at both IPs

---

### Phase 7: Integration and Polish (Days 12-13)
**Goal**: Complete feature parity and optimization

#### Tasks:
1. **Inter-task communication**
   - Embassy channels for config updates
   - Signals for state changes
   - Proper task synchronization

2. **Error handling**
   - Graceful WiFi failures
   - Storage error recovery
   - Display fallbacks

3. **Memory optimization**
   - Tune buffer sizes
   - Minimize allocations
   - Stack size optimization

4. **Validation and defaults**
   - Port all validation logic
   - Ensure Wokwi defaults work

#### Milestone 7: Feature-complete no_std nixie clock

---

### Phase 8: Testing and Documentation (Days 14-15)
**Goal**: Ensure reliability and maintainability

#### Tasks:
1. **Comprehensive testing**
   - Test all display modes
   - Verify configuration persistence
   - Test WiFi edge cases
   - Long-running stability test

2. **Performance comparison**
   - Binary size comparison
   - RAM usage analysis
   - Boot time measurement

3. **Documentation**
   - README for clock_no_std
   - Migration notes
   - Known differences/limitations

#### Final Milestone: Production-ready no_std nixie clock

---

## Implementation Order Rationale

1. **Hardware first**: Ensures the platform works before adding complexity
2. **Display before networking**: Core functionality without external dependencies  
3. **Storage before WiFi**: Need configuration to connect to networks
4. **WiFi before web server**: Web server needs network stack
5. **SNTP after basic networking**: Requires working UDP sockets
6. **Web server last**: Most complex component, needs everything else

## Testing Strategy

Each phase includes a concrete milestone that can be tested independently:
- Phase 1: Visual verification of display
- Phase 2: Button interaction testing
- Phase 3: Power cycle to verify persistence
- Phase 4: Network connectivity check
- Phase 5: Time accuracy verification
- Phase 6: Web UI functionality
- Phase 7: Full integration test
- Phase 8: Extended burn-in test

## Risk Mitigation

1. **Incremental development**: Each phase builds on previous work
2. **Early hardware validation**: Ensures platform compatibility
3. **Reuse existing drivers**: Minimize new code where possible
4. **Keep clock/ as reference**: Always have working version to compare

## Success Criteria

The no_std version is complete when:
- ✅ All features from original clock work identically
- ✅ Binary size is smaller than esp-idf-svc version
- ✅ Web interface is indistinguishable from original
- ✅ Configuration persists across power cycles
- ✅ Time accuracy matches original implementation
- ✅ Both network interfaces work simultaneously
- ✅ 24+ hour stability test passes

## Next Steps

1. Create `clock_no_std/` directory
2. Copy this plan to `clock_no_std/IMPLEMENTATION_PLAN.md`
3. Begin Phase 1 implementation
4. Track progress with git commits per milestone