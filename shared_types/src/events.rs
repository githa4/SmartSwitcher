#[derive(Debug, Clone)]
pub enum AppEvent {
    ShutdownRequested,
    Keyboard(KeyboardEvent),
}

#[derive(Debug, Clone)]
pub struct KeyboardEvent {
    pub vk_code: u32,
    pub scan_code: u32,
    pub flags: u32,
    pub is_key_down: bool,
}
