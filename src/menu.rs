use crate::configs::ConfigMenu;
use core::fmt;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_println::println;
use log::info;

pub enum MenuState {
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

impl MenuState {
    pub async fn got_line(&self, line: &str) -> Self {
        match self {
            MenuState::Idle(menu) => {
                if line.len() >= 1 && line.starts_with("m") {
                    info!("Enter menu");
                    return MenuState::Menu(menu);
                }
                return MenuState::Idle(menu);
            }
            MenuState::Menu(menu) => match line {
                "1" => {
                    list_entries(&menu).await;
                    return MenuState::Menu(menu);
                }
                "2" => {
                    println!("List values:");
                    return MenuState::SelectChange(menu);
                }
                "3" => {
                    return MenuState::ConfirmingReset(menu);
                }
                _ => return MenuState::Idle(menu),
            },
            MenuState::SelectChange(menu) => {
                let mut name = heapless::String::<32>::new();
                let _ = name.push_str(line);
                return MenuState::NewValue(menu, name);
            }
            MenuState::NewValue(menu, value) => {
                let mut unlocked = menu.lock().await;
                let _ = unlocked.store_entry(value, line);
                return MenuState::Menu(menu);
            }
            MenuState::ConfirmingReset(menu) => {
                if line.starts_with("y") {
                    info!("Reset flash storage");
                    let mut unlocked = menu.lock().await;
                    for entry in unlocked.entries.iter() {
                        let _ = unlocked.store_entry(entry.name, "");
                    }
                }
                return MenuState::Menu(menu);
            }
        }
    }

    pub async fn run_state(&self) {
        match self {
            MenuState::Idle(_) => {
                info!("Exit menu");
            }
            MenuState::Menu(_) => {
                println!("---------------------------");
                println!("Config menu, select option:");
                println!("1: list entries");
                println!("2: update value");
                println!("3: reset flash storage (useful if changing key)");
                println!("other: exit menu");
                println!("---------------------------");
                println!("");
            }
            MenuState::SelectChange(_) => {
                println!("Select entry name:");
            }
            MenuState::NewValue(menu, name) => {
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
            MenuState::ConfirmingReset(_) => {
                println!("Confirm flash reset with 'y':");
            }
        }
    }

    pub async fn secret_echo(&self) -> bool {
        if let MenuState::NewValue(menu, name) = self {
            let unlocked = menu.lock().await;
            if let Ok(entry) = unlocked.get_entry(name) {
                return entry.secret;
            }
        }
        false
    }
}

impl fmt::Debug for MenuState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MenuState::Idle(_) => f.debug_struct("State::Idle").finish(),
            MenuState::Menu(_) => f.debug_struct("State::Menu").finish(),
            MenuState::SelectChange(_) => f.debug_struct("State::SelectChange").finish(),
            MenuState::NewValue(_, entry) => f
                .debug_struct("State::NewValue")
                .field("entry", entry)
                .finish(),
            MenuState::ConfirmingReset(_) => f.debug_struct("State::ConfirmingReset").finish(),
        }
    }
}
