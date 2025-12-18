#[derive(Debug, Default, Clone)]
pub struct Platform;

impl Platform {
    pub fn new() -> Self {
        Self
    }

    #[cfg(target_os = "windows")]
    pub fn start_keyboard_hook(&self) -> anyhow::Result<windows::KeyboardHook> {
        windows::start_keyboard_hook()
    }
}

#[cfg(target_os = "windows")]
pub mod windows;
