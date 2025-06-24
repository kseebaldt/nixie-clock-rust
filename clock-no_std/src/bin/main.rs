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
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    timer::timg::TimerGroup,
};
use esp_println::println;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

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
    let mut data_pin = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    let mut clock_pin = Output::new(peripherals.GPIO17, Level::Low, OutputConfig::default());
    let mut latch_pin = Output::new(peripherals.GPIO18, Level::Low, OutputConfig::default());
    
    // Create nixie display
    let mut nixie_display = SimpleNixie::new(&mut data_pin, &mut clock_pin, &mut latch_pin);
    
    println!("Nixie display initialized");
    
    // Test display - show "1234" like the original
    println!("Displaying test pattern: 1234");
    nixie_display.display_digits(&[1, 2, 3, 4])
        .expect("Failed to display numbers");
    
    println!("Test pattern 1234 sent to shift register");
    
    // Test cycling through different numbers
    spawner.spawn(display_cycle_task()).ok();
    
    // Initialize WiFi (commented out for Phase 1)
    // let rng = esp_hal::rng::Rng::new(peripherals.RNG);
    // let timer1 = TimerGroup::new(peripherals.TIMG0);
    // let wifi_init = esp_wifi::init(timer1.timer0, rng, peripherals.RADIO_CLK)
    //     .expect("Failed to initialize WIFI/BLE controller");
    // let (mut _wifi_controller, _interfaces) = esp_wifi::wifi::new(&wifi_init, peripherals.WIFI)
    //     .expect("Failed to initialize WIFI controller");

    // Spawn display task
    spawner.spawn(display_task()).ok();

    // Main loop - keep running
    loop {
        Timer::after(Duration::from_secs(10)).await;
    }
}

#[embassy_executor::task]
async fn display_task() {
    // Simple task that just waits for now
    // In Phase 2, this will handle display updates
    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn display_cycle_task() {
    // We can't pass the display reference to a task easily with lifetimes
    // So let's just log that we would be cycling
    loop {
        Timer::after(Duration::from_secs(3)).await;
        println!("Would cycle display here in Phase 2");
    }
}
