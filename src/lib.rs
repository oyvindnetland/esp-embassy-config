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
    uart::{UartRx, UartTx},
};
use esp_println::println;
use log::info;

pub const READ_BUF_SIZE: usize = 64;

enum State {
    Idle(&'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>),
    Menu(&'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>),
    SelectChange(&'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>),
    NewValue(
        &'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>,
        heapless::String<32>,
    ),
    ConfirmingReset(&'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>),
}

async fn list_entries(menu: &'static Mutex<CriticalSectionRawMutex, ConfigMenu<'static>>) {
    println!("---------------------------");
    println!("List entries:");
    let mut unlocked = menu.lock().await;
    let mut cnt = 0;
    for entry in unlocked.entries.iter() {
        //let mut output = heapless::String::<{ value.n_blocks }>::new();
        let mut output = heapless::String::<32>::new();
        let v = unlocked.read_entry(entry.name, &mut output);
        if v.is_err() {
            let _ = output.push_str("-read failure-");
        } else {
            println!(
                "{}: Entry: {} size: {}: {}",
                cnt,
                entry.name,
                16 * entry.n_blocks,
                if entry.secret {
                    "********"
                } else {
                    output.as_str()
                },
            );
        }
        cnt += 1;
    }
    println!("---------------------------");
    println!("");
}

impl State {
    async fn got_line(&self, line: &str) -> Self {
        match self {
            State::Idle(menu) => {
                if line.len() >= 1 && line.starts_with("m") {
                    info!("Enter menu");
                    return State::Menu(menu);
                }
                return State::Idle(menu);
            }
            State::Menu(menu) => match line {
                "1" => {
                    list_entries(&menu).await;
                    return State::Menu(menu);
                }
                "2" => {
                    println!("List values:");
                    return State::SelectChange(menu);
                }
                "3" => {
                    return State::ConfirmingReset(menu);
                }
                _ => return State::Idle(menu),
            },
            State::SelectChange(menu) => {
                let mut name = heapless::String::<32>::new();
                let _ = name.push_str(line);
                return State::NewValue(menu, name);
            }
            State::NewValue(menu, value) => {
                let mut unlocked = menu.lock().await;
                let _ = unlocked.store_entry(value, line);
                return State::Menu(menu);
            }
            State::ConfirmingReset(menu) => {
                if line.starts_with("y") {
                    info!("Reset flash storage");
                    let mut unlocked = menu.lock().await;
                    for entry in unlocked.entries.iter() {
                        let _ = unlocked.store_entry(entry.name, "");
                    }
                }
                return State::Menu(menu);
            }
        }
    }

    async fn run_state(&self) {
        match self {
            State::Idle(_) => {
                info!("Exit menu");
            }
            State::Menu(_) => {
                println!("---------------------------");
                println!("Config menu, select option:");
                println!("1: list entries");
                println!("2: update value");
                println!("3: reset flash storage (useful if changing key)");
                println!("other: exit menu");
                println!("---------------------------");
                println!("");
            }
            State::SelectChange(_) => {
                println!("Select entry name:");
            }
            State::NewValue(menu, name) => {
                let unlocked = menu.lock().await;
                match unlocked.get_entry(name) {
                    Ok(entry) => {
                        println!("Update entry {}: {}", entry.name, entry.question);
                    }
                    Err(_) => {
                        println!("Entry not found: {}", name);
                    }
                }
            }
            State::ConfirmingReset(_) => {
                println!("Confirm flash reset with 'y':");
            }
        }
    }

    async fn secret_echo(&self) -> bool {
        if let State::NewValue(menu, name) = self {
            let unlocked = menu.lock().await;
            if let Ok(entry) = unlocked.get_entry(name) {
                return entry.secret;
            }
        }
        false
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Idle(_) => f.debug_struct("State::Idle").finish(),
            State::Menu(_) => f.debug_struct("State::Menu").finish(),
            State::SelectChange(_) => f.debug_struct("State::SelectChange").finish(),
            State::NewValue(_, entry) => f
                .debug_struct("State::NewValue")
                .field("entry", entry)
                .finish(),
            State::ConfirmingReset(_) => f.debug_struct("State::ConfirmingReset").finish(),
        }
    }
}
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
    let mut state = State::Idle(config_menu);
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
