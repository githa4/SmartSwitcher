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

    #[cfg(target_os = "windows")]
    pub fn get_active_lang_id(&self) -> anyhow::Result<u16> {
        windows::get_active_lang_id()
    }

    #[cfg(target_os = "windows")]
    pub fn set_layout_by_lang_id(
        &self,
        forbidden: &smart_switcher_shared_types::config::ForbiddenContextsConfig,
        lang_id: u16,
    ) -> anyhow::Result<bool> {
        windows::set_layout_by_lang_id(forbidden, lang_id)
    }

    #[cfg(target_os = "windows")]
    pub fn send_backspaces(
        &self,
        forbidden: &smart_switcher_shared_types::config::ForbiddenContextsConfig,
        count: usize,
    ) -> anyhow::Result<bool> {
        windows::send_backspaces(forbidden, count)
    }

    #[cfg(target_os = "windows")]
    pub fn send_unicode_text(
        &self,
        forbidden: &smart_switcher_shared_types::config::ForbiddenContextsConfig,
        text: &str,
    ) -> anyhow::Result<bool> {
        windows::send_unicode_text(forbidden, text)
    }

    #[cfg(not(target_os = "windows"))]
    pub fn switch_to_next_layout(
        &self,
        _forbidden: &smart_switcher_shared_types::config::ForbiddenContextsConfig,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }

    #[cfg(not(target_os = "windows"))]
    pub fn get_active_lang_id(&self) -> anyhow::Result<u16> {
        Ok(0)
    }

    #[cfg(not(target_os = "windows"))]
    pub fn set_layout_by_lang_id(
        &self,
        _forbidden: &smart_switcher_shared_types::config::ForbiddenContextsConfig,
        _lang_id: u16,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }

    #[cfg(not(target_os = "windows"))]
    pub fn send_backspaces(
        &self,
        _forbidden: &smart_switcher_shared_types::config::ForbiddenContextsConfig,
        _count: usize,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }

    #[cfg(not(target_os = "windows"))]
    pub fn send_unicode_text(
        &self,
        _forbidden: &smart_switcher_shared_types::config::ForbiddenContextsConfig,
        _text: &str,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }
}

#[cfg(target_os = "windows")]
pub mod windows;
