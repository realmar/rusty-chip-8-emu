use crate::vm::{config::Config, input::Input};
use ggez::input::keyboard::KeyCode;
use std::collections::{HashMap, HashSet};

pub struct GGEZInput {
    pressed_keys: Vec<u8>,
    mapping: HashMap<KeyCode, u8>,
}

impl GGEZInput {
    pub fn new(config: &Config) -> GGEZInput {
        let mut mapping = match config.get_rom_key_mapping() {
            Some(x) => x,
            None => &config.default_key_mapping,
        }
        .clone();

        for (keycode, value) in Config::get_default_key_mapping() {
            if mapping.contains_key(&keycode) == false {
                mapping.insert(keycode, value);
            }
        }

        GGEZInput {
            pressed_keys: Vec::with_capacity(16),
            mapping: mapping,
        }
    }

    pub fn update_keys(&mut self, keys: &HashSet<KeyCode>) {
        self.pressed_keys.clear();
        for code in keys.iter() {
            if let Some(key) = self.map_keycode(code) {
                self.pressed_keys.push(key);
            }
        }
    }

    fn map_keycode(&self, code: &KeyCode) -> Option<u8> {
        match self.mapping.get(code) {
            Some(key) => Some(*key),
            None => None,
        }
    }
}

impl Input for GGEZInput {
    fn is_pressed(&self, key: u8) -> bool {
        self.pressed_keys.contains(&key)
    }

    fn get_pressed_key(&self) -> Option<u8> {
        if self.pressed_keys.len() == 0 {
            None
        } else {
            Some(self.pressed_keys[0])
        }
    }
}
