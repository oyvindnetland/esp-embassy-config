#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_sync::mutex::Mutex;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_backtrace as _;
use esp_embassy_config::{
    config_init,
    configs::{ConfigEntry, ConfigMenu},
    key::make_key,
};
use esp_embassy_wifihelper::WifiStack;
use esp_hal::{
    aes::Aes,
    clock::CpuClock,
    sha::Sha,
    timer::timg::TimerGroup,
    uart::{Config, Uart},
};
use esp_wifi::wifi::ClientConfiguration;
use log::info;
use static_cell::StaticCell;

pub const READ_BUF_SIZE: usize = 64;

const STORAGE_KEY: &str = env!("STORAGE_KEY");

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    let mut config = esp_hal::Config::default();
    config.cpu_clock = CpuClock::max();
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(72 * 1024);

    let timg1 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timg1.timer0);

    // setup encryption
    let mut sha = Sha::new(peripherals.SHA);
    let encoded_key = make_key::<16>(&mut sha, STORAGE_KEY);
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

    // setup wifi
    static WIFI_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, ClientConfiguration, 1>> =
        StaticCell::new();
    let wifi_channel = WIFI_CHANNEL.init(Channel::new());

    // setup config menu
    static ENTRIES: StaticCell<[ConfigEntry; 1]> = StaticCell::new();
    static CONFIG_MENU: StaticCell<Mutex<CriticalSectionRawMutex, ConfigMenu>> = StaticCell::new();
    let entries = ENTRIES.init([ConfigEntry::new("test", 32, "Test test?", false)]);
    let config_menu = CONFIG_MENU.init(Mutex::new(ConfigMenu::new(
        entries,
        encoded_key,
        aes,
        wifi_channel.sender(),
    )));

    let wifi = WifiStack::new_connect_later(
        spawner,
        peripherals.WIFI,
        peripherals.TIMG0,
        peripherals.RNG,
        peripherals.RADIO_CLK,
        wifi_channel.receiver(),
    );

    // start config menu
    config_init(spawner, config_menu, uart_rx, uart_tx).await;
    info!("config started");

    let config = wifi.wait_for_connected().await.unwrap();
    info!("Wifi connected with IP: {}", config.address);
}
