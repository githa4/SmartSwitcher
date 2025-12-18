use anyhow::Context;
use async_trait::async_trait;
use smart_switcher_core::{Module, ModuleContext, ModuleHandle};
use smart_switcher_shared_types::{config::LayoutSwitcherConfig, AppEvent};
use tracing::{info, warn};

pub struct LayoutSwitcherModule {
    config: LayoutSwitcherConfig,
}

impl LayoutSwitcherModule {
    pub fn new(config: LayoutSwitcherConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Module for LayoutSwitcherModule {
    fn name(&self) -> &'static str {
        "layout_switcher"
    }

    async fn start(&self, ctx: ModuleContext) -> anyhow::Result<ModuleHandle> {
        let mut rx = ctx.bus.subscribe();
        let config = self.config.clone();
        let platform = ctx.platform.clone();

        let join = tokio::spawn(async move {
            info!(
                enabled = config.enabled,
                auto_detect = config.auto_detect,
                detect_threshold = config.detect_threshold,
                hotkey = %config.hotkey,
                "layout_switcher started",
            );

            let hotkey = config.hotkey.to_lowercase();
            if hotkey != "alt+shift" {
                warn!(hotkey = %config.hotkey, "unsupported hotkey, only alt+shift is supported in MVP");
            }

            let mut is_alt_down = false;
            let mut is_shift_down = false;
            let mut hotkey_fired = false;

            let mut word_keys: Vec<char> = Vec::new();

            let is_letter_vk = |vk: u32| (0x41..=0x5A).contains(&vk);
            let vk_to_letter = |vk: u32, shift: bool| {
                let base = (vk as u8 as char).to_ascii_lowercase();
                if shift {
                    base.to_ascii_uppercase()
                } else {
                    base
                }
            };

            let en_vowels = |s: &str| s.chars().any(|c| matches!(c, 'a'|'e'|'i'|'o'|'u'|'y'|'A'|'E'|'I'|'O'|'U'|'Y'));
            let ru_vowels = |s: &str| s.chars().any(|c| matches!(c, 'а'|'е'|'ё'|'и'|'о'|'у'|'ы'|'э'|'ю'|'я'|'А'|'Е'|'Ё'|'И'|'О'|'У'|'Ы'|'Э'|'Ю'|'Я'));

            let map_en_to_ru = |ch: char| -> char {
                match ch.to_ascii_lowercase() {
                    'q' => 'й', 'w' => 'ц', 'e' => 'у', 'r' => 'к', 't' => 'е', 'y' => 'н', 'u' => 'г', 'i' => 'ш', 'o' => 'щ', 'p' => 'з',
                    'a' => 'ф', 's' => 'ы', 'd' => 'в', 'f' => 'а', 'g' => 'п', 'h' => 'р', 'j' => 'о', 'k' => 'л', 'l' => 'д',
                    'z' => 'я', 'x' => 'ч', 'c' => 'с', 'v' => 'м', 'b' => 'и', 'n' => 'т', 'm' => 'ь',
                    other => other,
                }
            };

            let is_alt_vk = |vk: u32| matches!(vk, 0x12 | 0xA4 | 0xA5);
            let is_shift_vk = |vk: u32| matches!(vk, 0x10 | 0xA0 | 0xA1);

            loop {
                match rx.recv().await.context("event bus recv")? {
                    AppEvent::ShutdownRequested => {
                        info!("layout_switcher shutting down");
                        break;
                    }
                    AppEvent::Keyboard(ev) => {
                        if hotkey != "alt+shift" {
                            continue;
                        }

                        if is_alt_vk(ev.vk_code) {
                            is_alt_down = ev.is_key_down;
                        }
                        if is_shift_vk(ev.vk_code) {
                            is_shift_down = ev.is_key_down;
                        }

                        if !ev.is_key_down {
                            if !(is_alt_down && is_shift_down) {
                                hotkey_fired = false;
                            }
                            continue;
                        }

                        if is_alt_down && is_shift_down && !hotkey_fired {
                            hotkey_fired = true;
                            let switched = platform
                                .switch_to_next_layout(&config.forbidden_contexts)
                                .context("switch layout")?;
                            if switched {
                                info!("layout switched");
                            } else {
                                info!("layout switch skipped (forbidden or unavailable)");
                            }
                        }

                        if !config.auto_detect {
                            continue;
                        }

                        if is_alt_down {
                            continue;
                        }

                        match ev.vk_code {
                            0x08 => {
                                // Backspace
                                word_keys.pop();
                            }
                            0x20 | 0x0D => {
                                // Space / Enter
                                if word_keys.len() >= config.detect_threshold as usize {
                                    let lang = platform.get_active_lang_id().unwrap_or(0);
                                    let is_en = lang == 0x0409;
                                    let is_ru = lang == 0x0419;

                                    let typed: String = word_keys.iter().collect();

                                    if is_en {
                                        // EN (0x0409) -> RU (0x0419)
                                        let converted: String = typed.chars().map(map_en_to_ru).collect();

                                        if !en_vowels(&typed) && ru_vowels(&converted) {
                                            let _ = platform
                                                .set_layout_by_lang_id(&config.forbidden_contexts, 0x0419)
                                                .ok();
                                            let erased = platform
                                                .send_backspaces(&config.forbidden_contexts, word_keys.len())
                                                .unwrap_or(false);
                                            if erased {
                                                let injected = platform
                                                    .send_unicode_text(&config.forbidden_contexts, &converted)
                                                    .unwrap_or(false);
                                                if injected {
                                                    info!(from = %typed, to = %converted, "auto-detect corrected");
                                                }
                                            }
                                        }
                                    } else if is_ru {
                                        // RU (0x0419) -> EN (0x0409)
                                        // Тут `typed` — это физические латинские клавиши.
                                        // Если пользователь хотел английское слово, оно уже находится в `typed`.
                                        let would_be_ru: String = typed.chars().map(map_en_to_ru).collect();
                                        if en_vowels(&typed) && !ru_vowels(&would_be_ru) {
                                            let _ = platform
                                                .set_layout_by_lang_id(&config.forbidden_contexts, 0x0409)
                                                .ok();
                                            let erased = platform
                                                .send_backspaces(&config.forbidden_contexts, word_keys.len())
                                                .unwrap_or(false);
                                            if erased {
                                                let injected = platform
                                                    .send_unicode_text(&config.forbidden_contexts, &typed)
                                                    .unwrap_or(false);
                                                if injected {
                                                    info!(from = %typed, to = %typed, "auto-detect corrected");
                                                }
                                            }
                                        }
                                    }
                                }

                                word_keys.clear();
                            }
                            vk if is_letter_vk(vk) => {
                                // letters: collect physical key as latin char
                                let ch = vk_to_letter(vk, is_shift_down);
                                word_keys.push(ch);
                            }
                            _ => {
                                // delimiter / control
                                word_keys.clear();
                            }
                        }
                    }
                }
            }

            Ok(())
        });

        Ok(ModuleHandle::new(join))
    }
}
