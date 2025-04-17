#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_net::dns::DnsQueryType;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
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
use log::{error, info};

pub const READ_BUF_SIZE: usize = 64;

const STORAGE_KEY: &str = env!("STORAGE_KEY");

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

    // setup config menu
    let entries = &mut *mk_static!(
        [ConfigEntry; 2],
        [
            ConfigEntry::new("wifi_ssid", 32, "What is the wifi SSID?", false),
            ConfigEntry::new("wifi_pass", 64, "What is the wifi password?", true),
        ]
    );
    let config_menu = &*mk_static!(
        Mutex<CriticalSectionRawMutex, ConfigMenu>,
        Mutex::new(ConfigMenu::new(entries, encoded_key, aes))
    );

    // start config menu
    config_init(spawner, config_menu, uart_rx, uart_tx).await;

    // setup wifi
    let mut unlocked = config_menu.lock().await;
    let mut ssid = heapless::String::<32>::new();
    let res = unlocked.read_entry("wifi_ssid", &mut ssid);
    if res.is_err() {
        error!("Wifi SSID not set");
        loop {
            Timer::after(Duration::from_millis(1000)).await;
        }
    }

    let mut pass = heapless::String::<64>::new();
    let res = unlocked.read_entry("wifi_pass", &mut pass);
    if res.is_err() {
        error!("Wifi pass not set");
        loop {
            Timer::after(Duration::from_millis(1000)).await;
        }
    }
    drop(unlocked);

    let wifi = WifiStack::new(
        spawner,
        peripherals.WIFI,
        peripherals.TIMG0,
        peripherals.RNG,
        peripherals.RADIO_CLK,
        ssid,
        pass,
    );

    let config = wifi.wait_for_connected().await.unwrap();
    info!("Wifi connected with IP: {}", config.address);
}
