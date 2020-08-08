#[cfg(test)]
use mockall::automock;
use super::constants::{SCREEN_SIZE, SCREEN_SIZE_X};

pub type RawScreen = [u8; SCREEN_SIZE];

pub struct Snapshot {
    screen: RawScreen,
}

impl Snapshot {
    pub fn get_pixel(&self, x: usize, y: usize) -> u8 {
        self.screen[x + y * SCREEN_SIZE_X]
    }
}

#[cfg_attr(test, automock)]
pub trait Display : Send {
    fn get_screen(&self) -> &RawScreen;
    fn set_screen(&mut self, screen: &RawScreen);
    fn clear(&mut self);
    fn draw_sprite(&mut self, x: usize, y: usize, height: u8, data: &[u8]) -> DisplayState;
    fn get_snapshot(&self) -> Snapshot;
}

pub struct VmDisplay {
    screen: RawScreen,
}

impl VmDisplay {
    pub fn new() -> VmDisplay {
        VmDisplay {
            screen: [0; SCREEN_SIZE],
        }
    }
}

impl Display for VmDisplay {
    fn get_screen(&self) -> &RawScreen {
        &self.screen
    }

    fn set_screen(&mut self, screen: &RawScreen) {
        self.screen = screen.clone();
    }

    fn clear(&mut self) {
        for n in 0..self.screen.len() {
            self.screen[n] = 0;
        }
    }

    fn draw_sprite(&mut self, x: usize, y: usize, height: u8, data: &[u8]) -> DisplayState {
        let mut state = DisplayState::Unchanged;

        for sprite_y in 0..height as usize {
            let pixels = data[sprite_y];

            for sprite_x in 0..8 {
                if pixels & (0x80 >> sprite_x) != 0 {
                    let pixel_index = x + sprite_x + ((y + sprite_y) * SCREEN_SIZE_X);

                    if pixel_index < SCREEN_SIZE {
                        if self.screen[pixel_index] == 1 {
                            state = DisplayState::Changed;
                        }

                        self.screen[pixel_index] ^= 1;
                    }
                }
            }
        }

        state
    }

    fn get_snapshot(&self) -> Snapshot {
        Snapshot {
            screen: self.screen.clone(),
        }
    }
}

pub enum DisplayState {
    Changed,
    Unchanged,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand;

    fn new() -> VmDisplay {
        VmDisplay::new()
    }

    #[test]
    fn clear() {
        let mut d = new();

        for p in d.screen.iter_mut() {
            *p = rand::random::<u8>();
        }

        println!("{}", d.screen[0]);

        d.clear();

        for p in &d.screen {
            assert_eq!(*p, 0);
        }
    }
}
