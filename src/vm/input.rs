pub trait Input : Send {
    fn is_pressed(&self, key: u8) -> bool;
    fn get_pressed_key(&self) -> Option<u8>;
}
