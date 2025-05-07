# esp-embassy-config
A crate for storing data on persistent flash on esp32 devices. This can us used for
storing information like wifi ssid/password, some url to connect to etc. An uart menu
is used to list and update this config.

## `ConfigEntry`
A static arrays of `ConfigEntry` objects has to be passdd is passed to an `ConfigMenu` 
object. This means that the config setup is defined at compile time, which makes sense 
for a embedded project. In the examples this is done with StaticCell, which possibly could
be improved with a macro.

```cpp
// setup config menu
static ENTRIES: StaticCell<[ConfigEntry; 2]> = StaticCell::new();
static CONFIG_MENU: StaticCell<Mutex<CriticalSectionRawMutex, ConfigMenu>> = StaticCell::new();
let entries = ENTRIES.init([
    ConfigEntry::new("value", 16, "What is this value?", false),
    ConfigEntry::new("long_value", 32, "What is this other value?", true),
]);
let config_menu = CONFIG_MENU.init(Mutex::new(ConfigMenu::new(entries, encoded_key, aes)));
```
Notice that the "2" in the first line has to match the number of entries.

The `ConfigEntry` has:
- name, which is the identifier of the entry.
- n_blocks, which is the number of 16 bytes blocks that is used for storage.
- offset, which is calculated by the `ConfigMenu`.
- question, which is the question the menu system will ask when updating the entry.
- secret, which when true, will never display the content of the entry, just as stars

## Encryption
The information is AES encrypted before its written to flash. This is not intended to
be an absolute secure solution, but to prevent things like wifi password to be stored
in clear text on flash. The key for the encryption is a hash created from a supplied key
+ a hard coded salt. How the key is supplied is up to the user, importing it from an
enviroment variable is one possibility.

## Features

### wifi
The wifi feature adds some default entries to the config, and a menu item for connecting
to wifi. This makes it easy to store wifi password (relatively) safely on the device,
and connect using the `esp-embassy-wifihelper` crate. It is still possible to add your 
own custom entries, the wifi ones will just be there as well.

Adding the wifi dependencies adds significant build time and flash size (flash time), so
if its not needed, it should probably be skipped.

## Examples

### config_uart
Simple project for testing out how the menu works.

### config_wifi
A project that demonstrate the wifi feature, and connects to wifi using 
`esp-embassy-wifihelper` crate with the information stored in the config.
