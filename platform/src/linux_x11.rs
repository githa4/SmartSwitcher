use smart_switcher_shared_types::config::ForbiddenContextsConfig;

pub fn switch_to_next_layout(_forbidden: &ForbiddenContextsConfig) -> anyhow::Result<bool> {
    Ok(false)
}

pub fn get_active_lang_id() -> anyhow::Result<u16> {
    Ok(0)
}

pub fn set_layout_by_lang_id(
    _forbidden: &ForbiddenContextsConfig,
    _lang_id: u16,
) -> anyhow::Result<bool> {
    Ok(false)
}

pub fn send_backspaces(
    _forbidden: &ForbiddenContextsConfig,
    _count: usize,
) -> anyhow::Result<bool> {
    Ok(false)
}

pub fn send_unicode_text(
    _forbidden: &ForbiddenContextsConfig,
    _text: &str,
) -> anyhow::Result<bool> {
    Ok(false)
}

pub fn is_forbidden_context(_forbidden: &ForbiddenContextsConfig) -> anyhow::Result<bool> {
    Ok(false)
}
