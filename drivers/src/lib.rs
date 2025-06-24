#![cfg_attr(not(feature = "std"), no_std)]

extern crate embedded_hal as hal;

#[cfg(feature = "std")]
pub mod config;
pub mod debouncer;
pub mod nixie_display;
pub mod rgb_led;
pub mod shift_register;
#[cfg(feature = "std")]
pub mod storage;

// Re-export nixie_tube for no_std
pub use nixie_display as nixie_tube;
