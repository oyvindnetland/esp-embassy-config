use esp_hal::sha::{Sha, Sha256};

/**
 * Makes a hash from a provided key of arbitrarty length and a salt.
 * The length of the hash is determined by the generic argument
 */
pub fn make_key<const SZ: usize>(sha: &mut Sha, key: &str) -> [u8; SZ] {
    const SALT: &str = "SDFSMFOWRN¤#TIQN#T¤MT¤=R!E32r23r32r32fnwae";

    let mut hasher = sha.start::<Sha256>();
    let mut output = [0u8; SZ];

    let mut remaining = key.as_bytes();
    while remaining.len() > 0 {
        remaining = hasher.update(remaining).unwrap();
    }

    let mut remaining = SALT.as_bytes();
    while remaining.len() > 0 {
        remaining = hasher.update(remaining).unwrap();
    }

    hasher.finish(output.as_mut_slice()).unwrap();
    output
}
