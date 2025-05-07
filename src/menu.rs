use crate::configs::ConfigMenu;
use core::fmt;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_println::println;
#[cfg(feature = "wifi")]
use esp_wifi::wifi::ClientConfiguration;
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
        let mut output = heapless::String::<32>::new();
        let v = unlocked.read_entry(entry.name, &mut output);
        if v.is_err() {
            println!("{}: -read failure-", entry.name);
        } else {
            entry.print(cnt, output.as_str());
        }
        cnt += 1;
    }
    #[cfg(feature = "wifi")]
    {
        let mut output = heapless::String::<32>::new();
        let name = unlocked.wifi_ssid.name;
        let v = unlocked.read_entry(name, &mut output);
        if v.is_err() {
            println!("wifi_ssid: -read failure- name: {}", name);
        } else {
            unlocked.wifi_ssid.print(cnt, output.as_str());
        }
        let name = unlocked.wifi_pass.name;
        let v = unlocked.read_entry(name, &mut output);
        if v.is_err() {
            println!("wifi_pass: -read failure-");
        } else {
            unlocked.wifi_pass.print(cnt + 1, output.as_str());
        }
        let name = unlocked.wifi_autostart.name;
        let v = unlocked.read_entry(name, &mut output);
        if v.is_err() {
            println!("wifi_autostart: -read failure-");
        } else {
            unlocked.wifi_autostart.print(cnt + 1, output.as_str());
        }
    }
    println!("---------------------------");
    println!("");
}

fn print_menu() {
    println!("---------------------------");
    println!("Config menu, select option:");
    println!("1: show menu");
    println!("2: list entries");
    println!("3: update value");
    println!("4: reset flash storage (useful if changing key)");
    #[cfg(feature = "wifi")]
    println!("5: Connect to wifi");
    println!("other: exit menu");
    println!("---------------------------");
    println!("");
}

impl MenuState {
    pub async fn got_line(&self, line: &str) -> Self {
        match self {
            MenuState::Idle(menu) => {
                if line.len() >= 1 && line.starts_with("m") {
                    print_menu();
                    return MenuState::Menu(menu);
                }
                return MenuState::Idle(menu);
            }
            MenuState::Menu(menu) => match line {
                "1" => {
                    print_menu();
                    return MenuState::Menu(menu);
                }
                "2" => {
                    list_entries(&menu).await;
                    return MenuState::Menu(menu);
                }
                "3" => {
                    println!("List values:");
                    return MenuState::SelectChange(menu);
                }
                "4" => {
                    return MenuState::ConfirmingReset(menu);
                }
                #[cfg(feature = "wifi")]
                "5" => {
                    let mut client_config = ClientConfiguration::default();

                    let mut unlocked = menu.lock().await;
                    let res = unlocked.read_entry("wifi_ssid", &mut client_config.ssid);
                    if res.is_err() {
                        println!("Failed to connect to wifi, no SSID set");
                        return MenuState::Menu(menu);
                    }
                    let res = unlocked.read_entry("wifi_pass", &mut client_config.password);
                    if res.is_err() {
                        println!("Failed to connect to wifi, no Pass set");
                        return MenuState::Menu(menu);
                    }

                    unlocked.wifi_sender.send(client_config).await;

                    return MenuState::Menu(menu);
                }
                _ => return MenuState::Idle(menu),
            },
            MenuState::SelectChange(menu) => {
                let mut name = heapless::String::<32>::new();
                if let Ok(index) = line.parse::<usize>() {
                    let unlocked = menu.lock().await;
                    let entry = unlocked.get_entry_index(index);
                    if entry.is_ok() {
                        let _ = name.push_str(entry.unwrap().name);
                        return MenuState::NewValue(menu, name);
                    }
                }
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
            MenuState::Menu(_) => {}
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
