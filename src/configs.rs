#[cfg(feature = "wifi")]
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Sender;
use embedded_storage::ReadStorage;
use embedded_storage::Storage;
use esp_hal::aes::{Aes, Key, Mode};
use esp_println::println;
use esp_storage::FlashStorage;
#[cfg(feature = "wifi")]
use esp_wifi::wifi::ClientConfiguration;

pub struct ConfigMenu<'a> {
    pub entries: &'a [ConfigEntry<'a>],
    #[cfg(feature = "wifi")]
    pub wifi_ssid: ConfigEntry<'a>,
    #[cfg(feature = "wifi")]
    pub wifi_pass: ConfigEntry<'a>,
    #[cfg(feature = "wifi")]
    pub wifi_autostart: ConfigEntry<'a>,
    #[cfg(feature = "wifi")]
    pub wifi_sender: Sender<'static, CriticalSectionRawMutex, ClientConfiguration, 1>,
    key: [u8; 16],
    aes: Aes<'a>,
    storage: FlashStorage,
}

impl<'a> ConfigMenu<'a> {
    pub fn new(
        values: &'a mut [ConfigEntry<'a>],
        key: [u8; 16],
        aes: Aes<'a>,
        #[cfg(feature = "wifi")] wifi_sender: Sender<
            'static,
            CriticalSectionRawMutex,
            ClientConfiguration,
            1,
        >,
    ) -> Self {
        let mut offset = 0;
        for value in values.iter_mut() {
            value.offset = offset;
            offset += 16 * (value.n_blocks as u32);
        }

        let mut wifi_ssid = ConfigEntry::new("wifi_ssid", 32, "Wifi SSID", false);
        wifi_ssid.offset = offset;
        let mut wifi_pass = ConfigEntry::new("wifi_pass", 64, "Wifi Password", true);
        wifi_pass.offset = offset + 32;
        let mut wifi_autostart = ConfigEntry::new(
            "wifi_autostart",
            32,
            "Set to 'yes' if wifi should be connected automatically at boot",
            false,
        );
        wifi_autostart.offset = offset + 32 + 64;

        let mut config_menu = Self {
            entries: values,
            #[cfg(feature = "wifi")]
            wifi_ssid,
            #[cfg(feature = "wifi")]
            wifi_pass,
            #[cfg(feature = "wifi")]
            wifi_sender,
            #[cfg(feature = "wifi")]
            wifi_autostart,
            key,
            aes,
            storage: FlashStorage::new(),
        };
        config_menu
    }

    #[cfg(feature = "wifi")]
    pub async fn autostart_wifi(&mut self) {
        use embassy_time::{Duration, Timer};

        let mut autostart = heapless::String::<32>::new();
        if let Ok(_) =
            self.wifi_autostart
                .read(&self.key, &mut self.aes, &mut self.storage, &mut autostart)
        {
            if autostart == "yes" {
                let mut ok = true;

                let mut ssid = heapless::String::<32>::new();
                if let Err(_) =
                    self.wifi_ssid
                        .read(&self.key, &mut self.aes, &mut self.storage, &mut ssid)
                {
                    ok = false;
                }

                let mut pass = heapless::String::<64>::new();
                if let Err(_) =
                    self.wifi_pass
                        .read(&self.key, &mut self.aes, &mut self.storage, &mut pass)
                {
                    ok = false;
                }

                if ok {
                    let client_config = ClientConfiguration {
                        ssid,
                        password: pass,
                        ..Default::default()
                    };
                    let _ = self.wifi_sender.send(client_config).await;
                }
            }
        }
    }

    pub fn get_entry_index(&self, index: usize) -> Result<&ConfigEntry, ()> {
        if index >= self.entries.len() {
            return Err(());
        }
        Ok(&self.entries[index])
    }

    pub fn get_entry(&self, name: &str) -> Result<&ConfigEntry, ()> {
        for entry in self.entries.iter() {
            if entry.check_name(name) {
                return Ok(&entry);
            }
        }

        #[cfg(feature = "wifi")]
        {
            if name == "wifi_ssid" {
                return Ok(&self.wifi_ssid);
            }
            if name == "wifi_pass" {
                return Ok(&self.wifi_pass);
            }
            if name == "wifi_autostart" {
                return Ok(&self.wifi_autostart);
            }
        }
        Err(())
    }

    pub fn store_entry(&mut self, name: &str, input: &str) -> Result<(), ()> {
        for entry in self.entries.iter() {
            if entry.check_name(name) {
                return entry.store(&self.key, &mut self.aes, &mut self.storage, input);
            }
        }
        #[cfg(feature = "wifi")]
        if name == "wifi_ssid" {
            return self
                .wifi_ssid
                .store(&self.key, &mut self.aes, &mut self.storage, input);
        }
        #[cfg(feature = "wifi")]
        if name == "wifi_pass" {
            return self
                .wifi_pass
                .store(&self.key, &mut self.aes, &mut self.storage, input);
        }
        #[cfg(feature = "wifi")]
        if name == "wifi_autostart" {
            return self
                .wifi_autostart
                .store(&self.key, &mut self.aes, &mut self.storage, input);
        }

        Err(())
    }

    pub fn read_entry<const MAX_SZ: usize>(
        &mut self,
        name: &str,
        output: &mut heapless::String<MAX_SZ>,
    ) -> Result<(), ()> {
        for value in self.entries.iter() {
            if value.check_name(name) {
                return value.read(&self.key, &mut self.aes, &mut self.storage, output);
            }
        }

        #[cfg(feature = "wifi")]
        {
            if name == "wifi_ssid" {
                return self
                    .wifi_ssid
                    .read(&self.key, &mut self.aes, &mut self.storage, output);
            }
            if name == "wifi_pass" {
                return self
                    .wifi_pass
                    .read(&self.key, &mut self.aes, &mut self.storage, output);
            }
            if name == "wifi_autostart" {
                return self.wifi_autostart.read(
                    &self.key,
                    &mut self.aes,
                    &mut self.storage,
                    output,
                );
            }
        }
        Err(())
    }
}

#[derive(Debug)]
pub struct ConfigEntry<'a> {
    pub name: &'a str,
    pub n_blocks: usize, // number of blocks of 16 bytes
    pub offset: u32,
    pub question: &'a str,
    pub secret: bool,
}

impl<'a> ConfigEntry<'a> {
    pub fn new(name: &'a str, max_len: usize, question: &'a str, secret: bool) -> Self {
        Self {
            name,
            n_blocks: max_len.div_ceil(16),
            offset: 0,
            question,
            secret,
        }
    }

    pub fn print(&self, cnt: i32, output: &str) {
        if self.secret {
            println!(
                "{}: Entry: {} size: -/{}: ********",
                cnt,
                self.name,
                16 * self.n_blocks
            );
        } else {
            println!(
                "{}: Entry: {} size: {}/{}: {}",
                cnt,
                self.name,
                output.len(),
                16 * self.n_blocks,
                output,
            );
        }
    }

    fn check_name(&self, name: &str) -> bool {
        name == self.name
    }

    pub fn store(
        &self,
        key: &[u8; 16],
        aes: &mut Aes,
        storage: &mut FlashStorage,
        value: &str,
    ) -> Result<(), ()> {
        if value.len() > 16 * self.n_blocks {
            return Err(());
        }

        let mut cur_offset = self.offset;
        for i in 0..self.n_blocks {
            let start = 16 * i;
            let mut end = 16 * (i + 1);
            if end > value.len() {
                end = value.len();
            }

            let sub_str = if start >= value.len() {
                ""
            } else {
                &value[start..end]
            };

            let mut block = [0_u8; 16];
            block[..sub_str.len()].copy_from_slice(sub_str.as_bytes());
            let k: Key = (*key).into();
            aes.process(&mut block, Mode::Encryption128, k);

            let _ = storage.write(0x9000 + cur_offset, &block);
            cur_offset += 16;
        }
        Ok(())
    }

    pub fn read<const MAX_SZ: usize>(
        &self,
        key: &[u8; 16],
        aes: &mut Aes,
        storage: &mut FlashStorage,
        output: &mut heapless::String<MAX_SZ>,
    ) -> Result<(), ()> {
        output.clear();
        let mut cur_offset = self.offset;
        for _ in 0..self.n_blocks {
            let mut block = [0_u8; 16];
            let _ = storage.read(0x9000 + cur_offset, &mut block);
            cur_offset += 16;

            let k: Key = (*key).into();
            aes.process(&mut block, Mode::Decryption128, k);

            if !block.is_ascii() {
                return Err(());
            }

            for byte in block {
                if byte == 0 {
                    return Ok(());
                }
                let res = output.push(byte as char);
                if res.is_err() {
                    return Err(());
                }
            }
        }
        Ok(())
    }
}
