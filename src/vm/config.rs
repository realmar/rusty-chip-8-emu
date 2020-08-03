use ggez::input::keyboard::KeyCode;

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use log::{LevelFilter, warn};

pub type KeyMapping = HashMap<KeyCode, u8>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebuggerKeyMapping {
    pub toggle_break: KeyCode,
    pub step_previous: KeyCode,
    pub step_next: KeyCode,
    pub print_registers: KeyCode,
    pub print_stack: KeyCode,
    pub print_timers: KeyCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebuggerConfig {
    pub enable: bool,
    pub key_mapping: DebuggerKeyMapping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralKeyMapping {
    pub restart_vm: KeyCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub hz: u128,
    pub beep_frequency: f64,
    pub rom: String,
    pub general_key_mapping: GeneralKeyMapping,
    pub default_key_mapping: KeyMapping,
    pub rom_key_mappings: HashMap<String, KeyMapping>,
    pub debugger: DebuggerConfig,
    pub log_level: LevelFilter,
}

impl Config {
    pub fn load() -> Result<Config, Box<dyn Error + 'static>> {
        const PATH: &'static str = "config.yml";
        let read_result = fs::read_to_string(PATH);

        match read_result {
            Ok(yaml) => Ok(serde_yaml::from_str::<Config>(&yaml)?),
            Err(..) => {
                let config = Config::default();
                let yaml = serde_yaml::to_string(&config)?;

                if let Err(err) = fs::write(PATH, yaml) {
                    warn!("Failed to write default config to {}: {}", PATH, err);
                }

                Ok(config)
            }
        }
    }

    pub fn get_rom_keymapping(&self) -> Option<&KeyMapping> {
        let filename = Path::new(&self.rom).file_name()?.to_str()?;
        self.rom_key_mappings.get(filename)
    }

    pub fn get_default_key_mapping() -> KeyMapping {
        let mut map = HashMap::with_capacity(16);
        map.insert(KeyCode::Key0, 0);
        map.insert(KeyCode::Key1, 1);
        map.insert(KeyCode::Key2, 2);
        map.insert(KeyCode::Key3, 3);
        map.insert(KeyCode::Key4, 4);
        map.insert(KeyCode::Key5, 5);
        map.insert(KeyCode::Key6, 6);
        map.insert(KeyCode::Key7, 7);
        map.insert(KeyCode::Key8, 8);
        map.insert(KeyCode::Key9, 9);
        map.insert(KeyCode::A, 0xA);
        map.insert(KeyCode::B, 0xB);
        map.insert(KeyCode::C, 0xC);
        map.insert(KeyCode::D, 0xD);
        map.insert(KeyCode::E, 0xE);
        map.insert(KeyCode::F, 0xF);

        map
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            hz: 60,
            beep_frequency: 440.,
            rom: String::from("roms/PONG2"),
            general_key_mapping: GeneralKeyMapping {
                restart_vm: KeyCode::F5,
            },
            default_key_mapping: Config::get_default_key_mapping(),
            rom_key_mappings: HashMap::<String, KeyMapping>::new(),
            debugger: DebuggerConfig {
                enable: false,
                key_mapping: DebuggerKeyMapping {
                    toggle_break: KeyCode::F1,
                    step_previous: KeyCode::F2,
                    step_next: KeyCode::F3,
                    print_registers: KeyCode::F4,
                    print_stack: KeyCode::F6,
                    print_timers: KeyCode::F7,
                },
            },
            log_level: LevelFilter::Trace
        }
    }
}
