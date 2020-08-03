use super::constants::*;

pub type RawScreen = [u8; SCREEN_SIZE];

pub struct Display {
    screen: RawScreen,
}

impl Display {
    pub fn new() -> Display {
        Display {
            screen: [0; SCREEN_SIZE],
        }
    }

    pub(super) fn get_screen(&self) -> &RawScreen {
        &self.screen
    }

    pub(super) fn set_screen(&mut self, screen: &RawScreen) {
        self.screen = screen.clone();
    }

    pub(super) fn clear(&mut self) {
        for n in 0..self.screen.len() {
            self.screen[n] = 0;
        }
    }

    pub(super) fn draw_sprite(&mut self, x: usize, y: usize, height: u8, data: &[u8]) -> DisplayState {
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

    pub fn get_pixel(&self, x: usize, y: usize) -> u8 {
        self.screen[x + y * SCREEN_SIZE_X]
    }
}

pub enum DisplayState {
    Changed,
    Unchanged,
}
