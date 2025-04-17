#![no_std]

pub mod configs;
pub mod key;

use configs::ConfigMenu;
use core::fmt;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal::{
    Async,
    uart::{Uart, UartRx, UartTx},
};

pub const READ_BUF_SIZE: usize = 64;

enum State {
    Idle(&'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>),
    Menu(&'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>),
    ListValues(&'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>),
    SelectChange(&'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>),
    NewValue(
        &'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>,
        heapless::String<32>,
    ),
}

impl State {
    async fn got_line(&self, line: &str) -> Self {
        match self {
            State::Idle(menu) => {
                if line.len() >= 1 && line.starts_with("m") {
                    return State::Menu(menu);
                }
                return State::Idle(menu);
            }
            State::Menu(menu) => match line {
                "1" => return State::ListValues(menu),
                "2" => {
                    esp_println::println!("List values:");
                    return State::SelectChange(menu);
                }
                _ => return State::Idle(menu),
            },
            State::ListValues(menu) => return State::Menu(menu),
            State::SelectChange(menu) => {
                let mut str = heapless::String::<32>::new();
                let _ = str.push_str(line);
                return State::NewValue(menu, str);
            }
            State::NewValue(menu, value) => {
                let mut unlocked = menu.lock().await;
                let _ = unlocked.store_entry(value, line);
                return State::Menu(menu);
            }
        }
    }

    async fn run_state(&self) {
        match self {
            State::Idle(_) => {}
            State::Menu(_) => {
                esp_println::println!("---------------------------");
                esp_println::println!("Config menu, select option:");
                esp_println::println!("1: list values");
                esp_println::println!("2: update value");
                esp_println::println!("other: exit menu");
                esp_println::println!("---------------------------");
                esp_println::println!("");
            }
            State::ListValues(menu) => {
                esp_println::println!("---------------------------");
                esp_println::println!("List values:");
                let mut unlocked = menu.lock().await;
                let mut cnt = 0;
                for entry in unlocked.entries.iter() {
                    //let mut output = heapless::String::<{ value.n_blocks }>::new();
                    let mut output = heapless::String::<32>::new();
                    let v = unlocked.read_entry(entry.name, &mut output);
                    if v.is_err() {
                        let _ = output.push_str("-read failure-");
                    } else {
                        esp_println::println!(
                            "{}: Value: {} size: {}: {}",
                            cnt,
                            entry.name,
                            entry.n_blocks,
                            output
                        );
                    }
                    cnt += 1;
                }
                esp_println::println!("---------------------------");
                esp_println::println!("");
            }
            State::SelectChange(_) => {
                esp_println::println!("Select entry name:");
            }
            State::NewValue(_, entry) => {
                esp_println::println!("Set new value for {}:", entry);
            }
        }
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Idle(_) => f.debug_struct("State::Idle").finish(),
            State::Menu(_) => f.debug_struct("State::Menu").finish(),
            State::ListValues(_) => f.debug_struct("State::ListValues").finish(),
            State::SelectChange(_) => f.debug_struct("State::SelectChange").finish(),
            State::NewValue(_, entry) => f
                .debug_struct("State::NewValue")
                .field("entry", entry)
                .finish(),
        }
    }
}
pub async fn config_init(
    spawner: Spawner,
    config_menu: &'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>,
    mut rx: UartRx<'static, Async>,
    mut tx: UartTx<'static, Async>,
) {
    spawner.spawn(run_config_menu(config_menu, rx, tx)).ok();
}

#[embassy_executor::task]
async fn run_config_menu(
    config_menu: &'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>,
    mut rx: UartRx<'static, Async>,
    mut tx: UartTx<'static, Async>,
) {
    let mut state = State::Idle(config_menu);
    loop {
        if let Ok(line) = get_line::<32>(&mut rx, &mut tx).await {
            state = state.got_line(line.as_str()).await;
            state.run_state().await;
        }
    }
}

async fn get_line<const SZ: usize>(
    rx: &mut UartRx<'static, Async>,
    tx: &mut UartTx<'static, Async>,
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

                let _ = tx.write_async(&buf).await;
                let _ = tx.flush_async().await;
                if buf[0] == 13 {
                    return Ok(line);
                }

                let _ = line.push(buf[0] as char);
                if line.len() == SZ {
                    esp_println::println!("Reached {} characters", SZ);
                    return Ok(line);
                }
            }
            Err(_) => return Err(()),
        }
    }
}
