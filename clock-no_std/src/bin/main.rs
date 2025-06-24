#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use clock_no_std::simple_nixie::SimpleNixie;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embassy_futures::yield_now;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    timer::timg::TimerGroup,
};
use esp_println::println;
use chrono::{DateTime, Timelike};
use static_cell::StaticCell;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Global signal for button press events
static BUTTON_PRESS_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();


#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.4.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    // Initialize nixie display pins
    // Based on the ACTUAL original clock configuration:
    // - Data pin: GPIO16
    // - Clock pin: GPIO17  
    // - Latch pin: GPIO18
    let data_pin = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    let clock_pin = Output::new(peripherals.GPIO17, Level::Low, OutputConfig::default());
    let latch_pin = Output::new(peripherals.GPIO18, Level::Low, OutputConfig::default());
    
    // Store pins in static cells for task use
    static DATA_PIN: StaticCell<Output<'static>> = StaticCell::new();
    static CLOCK_PIN: StaticCell<Output<'static>> = StaticCell::new();
    static LATCH_PIN: StaticCell<Output<'static>> = StaticCell::new();
    
    let data_pin = DATA_PIN.init(data_pin);
    let clock_pin = CLOCK_PIN.init(clock_pin);
    let latch_pin = LATCH_PIN.init(latch_pin);
    
    // Set up button pin for async usage
    let config = InputConfig::default().with_pull(Pull::Up);
    let button = Input::new(peripherals.GPIO19, config);
    
    println!("Button pin configured on GPIO19");
    
    // Store button in static cell for button task
    static BUTTON_PIN: StaticCell<Input<'static>> = StaticCell::new();
    let button = BUTTON_PIN.init(button);
    
    println!("Nixie display and button initialized");
    
    // Initialize WiFi (commented out for Phase 1)
    // let rng = esp_hal::rng::Rng::new(peripherals.RNG);
    // let timer1 = TimerGroup::new(peripherals.TIMG0);
    // let wifi_init = esp_wifi::init(timer1.timer0, rng, peripherals.RADIO_CLK)
    //     .expect("Failed to initialize WIFI/BLE controller");
    // let (mut _wifi_controller, _interfaces) = esp_wifi::wifi::new(&wifi_init, peripherals.WIFI)
    //     .expect("Failed to initialize WIFI controller");

    // Spawn display update task that handles mode cycling
    if spawner.spawn(display_update_task(data_pin, clock_pin, latch_pin)).is_err() {
        println!("Failed to spawn display task");
    }
    
    // Spawn button handling task using Embassy async approach
    if spawner.spawn(button_task(button)).is_err() {
        println!("Failed to spawn button task");
    }

    // Main loop - keep running (use yield_now since Timer::after has issues)
    loop {
        for _ in 0..200000 { // Rough equivalent to 10 second delay
            yield_now().await;
        }
    }
}

// Embassy-style async button task 
#[embassy_executor::task]
async fn button_task(button: &'static mut Input<'static>) {
    loop {
        // Wait for button press (falling edge)
        button.wait_for_falling_edge().await;
        
        println!("Button pressed - signaling mode change");
        BUTTON_PRESS_SIGNAL.signal(());
        
        // Simple debounce delay
        for _ in 0..1000 {
            yield_now().await;
        }
    }
}

#[embassy_executor::task]
async fn display_update_task(
    data_pin: &'static mut Output<'static>,
    clock_pin: &'static mut Output<'static>, 
    latch_pin: &'static mut Output<'static>
) {
    let mut nixie_display = SimpleNixie::new(data_pin, clock_pin, latch_pin);
    
    // Initial test display
    if let Err(_e) = nixie_display.test_pattern() {
        println!("Test pattern failed");
    }
    
    let mut counter = 0u64;
    
    loop {
        
        // Check for button press signal
        if BUTTON_PRESS_SIGNAL.try_take().is_some() {
            println!("*** MODE CHANGE RECEIVED IN DISPLAY TASK! ***");
            nixie_display.next_mode();
            println!("Mode cycled, new mode: {:?}", nixie_display.get_mode());
        }
        
        // Update display every 2 seconds with incrementing fake time
        if counter % 40 == 0 { // Every 2 seconds (40 * 50ms)
            // Use fake time that increments
            let fake_timestamp = 1640995200 + (counter / 40) * 60; // Add 1 minute each update
            if let Some(fake_time) = DateTime::from_timestamp(fake_timestamp as i64, 0) {
                if let Err(_e) = nixie_display.update(&fake_time) {
                    println!("Display update failed");
                }
            }
        }
        
        counter += 1;
        
        // Use a simple delay loop instead of Timer::after since that's broken
        for _ in 0..50 {
            yield_now().await;
        }
    }
}
