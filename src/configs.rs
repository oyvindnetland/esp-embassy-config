use embedded_storage::ReadStorage;
use embedded_storage::Storage;
use esp_hal::aes::{Aes, Key, Mode};
use esp_storage::FlashStorage;
use log::info;

pub struct ConfigMenu<'a> {
    pub entries: &'a [ConfigEntry<'a>],
    key: [u8; 16],
    aes: Aes<'a>,
    storage: FlashStorage,
}

impl<'a> ConfigMenu<'a> {
    pub fn new(values: &'a mut [ConfigEntry<'a>], key: [u8; 16], aes: Aes<'a>) -> Self {
        let mut offset = 0;
        for value in values.iter_mut() {
            value.offset = offset;
            offset += 16 * (value.n_blocks as u32);
        }

        Self {
            entries: values,
            key,
            aes,
            storage: FlashStorage::new(),
        }
    }

    pub fn get_entry(&self, name: &str) -> Result<&ConfigEntry, ()> {
        for entry in self.entries.iter() {
            if entry.check_name(name) {
                return Ok(&entry);
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
