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

    #[cfg(target_os = "windows")]
    pub fn switch_to_next_layout(
        &self,
        forbidden: &smart_switcher_shared_types::config::ForbiddenContextsConfig,
    ) -> anyhow::Result<bool> {
        windows::switch_to_next_layout(forbidden)
    }

    #[cfg(not(target_os = "windows"))]
    pub fn switch_to_next_layout(
        &self,
        _forbidden: &smart_switcher_shared_types::config::ForbiddenContextsConfig,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }
}

#[cfg(target_os = "windows")]
pub mod windows;
