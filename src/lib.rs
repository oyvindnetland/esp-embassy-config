#![no_std]

pub mod configs;
pub mod key;
mod menu;

use configs::ConfigMenu;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal::{
    Async,
    uart::{UartRx, UartTx},
};
use log::info;
use menu::MenuState;

pub const READ_BUF_SIZE: usize = 64;

pub async fn config_init(
    spawner: Spawner,
    config_menu: &'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>,
    rx: UartRx<'static, Async>,
    tx: UartTx<'static, Async>,
) {
    spawner.spawn(run_config_menu(config_menu, rx, tx)).ok();
}

#[embassy_executor::task]
async fn run_config_menu(
    config_menu: &'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>,
    mut rx: UartRx<'static, Async>,
    mut tx: UartTx<'static, Async>,
) {
    #[cfg(feature = "wifi")]
    {
        let mut c = config_menu.lock().await;
        c.autostart_wifi().await;
    }

    let mut state = MenuState::Idle(config_menu);
    loop {
        let secret_echo = state.secret_echo().await;
        if let Ok(line) = get_line::<32>(&mut rx, &mut tx, secret_echo).await {
            state = state.got_line(line.as_str()).await;
            state.run_state().await;
        }
    }
}

async fn get_line<const SZ: usize>(
    rx: &mut UartRx<'static, Async>,
    tx: &mut UartTx<'static, Async>,
    secret_echo: bool,
) -> Result<heapless::String<SZ>, ()> {
    let mut buf: [u8; 1] = [0; 1];
    let mut line = heapless::String::<SZ>::new();
    loop {
        let res = rx.read_async(buf.as_mut_slice()).await;
        match res {
            Ok(len) => {
                if len < 1 {
                    continue;
                }

                if buf[0] == 13 {
                    let _ = tx.write_async(&buf).await;
                    let _ = tx.flush_async().await;
                    return Ok(line);
                }

                if secret_echo {
                    let _ = tx.write_async("*".as_bytes()).await;
                } else {
                    let _ = tx.write_async(&buf).await;
                }
                let _ = tx.flush_async().await;

                let _ = line.push(buf[0] as char);
                if line.len() == SZ {
                    info!("Reached {} characters", SZ);
                    return Ok(line);
                }
            }
            Err(_) => return Err(()),
        }
    }
}
