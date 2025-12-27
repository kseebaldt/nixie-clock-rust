# AGENTS.md

## Build/Test/Lint Commands
```bash
cargo build --release                                              # Build
cargo fmt --all -- --check                                         # Format check
cargo clippy --all-targets --all-features --workspace -- -D warnings  # Lint
cargo test --workspace                                             # Run all tests
cargo test -p drivers -- test_name                                 # Single test in drivers crate
```

## Code Style
- **Imports**: External crates first, std library second, local/crate imports last
- **Error handling**: `thiserror` for library errors, `anyhow::Result` for application code
- **Naming**: snake_case functions/variables, PascalCase types, SCREAMING_SNAKE_CASE constants
- **Tests**: Place in same file under `#[cfg(test)] mod tests`, use `it_` prefix for test names
- **Embedded**: Use generics over `embedded-hal` traits, support `no_std` where applicable
- **Linting**: All clippy warnings are errors (`-D warnings`)

## Project Structure
- `clock/` - Main ESP32 application (esp-idf-svc, WiFi, HTTP server)
- `drivers/` - Hardware abstraction library (nixie display, shift register, RGB LED)
- `testing/` - Mock implementations for testing
- `clock-no_std/` - Alternative no_std implementation using embassy async
