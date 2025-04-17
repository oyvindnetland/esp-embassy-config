#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_embassy_config::{
    config_init,
    configs::{ConfigEntry, ConfigMenu},
    key::make_key,
};
use esp_hal::{
    aes::Aes,
    sha::Sha,
    timer::timg::TimerGroup,
    uart::{Config, Uart},
};
use log::info;

pub const READ_BUF_SIZE: usize = 64;

const KEY: &str = "BNMIKUJYHGFDEWRGYJ";

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // setup embassy
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    // setup encryption
    let mut sha = Sha::new(peripherals.SHA);
    let encoded_key = make_key::<16>(&mut sha, KEY);
    let aes = Aes::<'static>::new(peripherals.AES);

    // setup uart
    let (tx_pin, rx_pin) = (peripherals.GPIO21, peripherals.GPIO20);
    let config = Config::default().with_rx_fifo_full_threshold(READ_BUF_SIZE as u16);
    let uart0 = Uart::new(peripherals.UART0, config)
        .unwrap()
        .with_tx(tx_pin)
        .with_rx(rx_pin)
        .into_async();
    let (uart_rx, uart_tx) = uart0.split();
    Timer::after(Duration::from_millis(100)).await;

    // setup config menu
    let entries = &mut *mk_static!(
        [ConfigEntry; 2],
        [
            ConfigEntry::new("value", 16),
            ConfigEntry::new("long_value", 32),
        ]
    );
    let config_menu = &*mk_static!(
        Mutex<CriticalSectionRawMutex, ConfigMenu>,
        Mutex::new(ConfigMenu::new(entries, encoded_key, aes))
    );

    // start config menu
    info!("Starting config menu");
    config_init(spawner, config_menu, uart_rx, uart_tx).await;
}
