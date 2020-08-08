use super::constants::TIMER_DURATION_NANO;

#[derive(Clone)]
pub struct Timer(u128);

impl Timer {
    pub fn new() -> Timer {
        Timer(0u128)
    }

    pub fn get_scaled(&self) -> u8 {
        (self.0 / TIMER_DURATION_NANO) as u8
    }

    pub fn set_scaled(&mut self, value: u8) {
        self.0 = value as u128 * TIMER_DURATION_NANO;
    }

    pub fn get(&self) -> u128 {
        self.0
    }

    pub fn get_mut(&mut self) -> &mut u128 {
        &mut self.0
    }
}
