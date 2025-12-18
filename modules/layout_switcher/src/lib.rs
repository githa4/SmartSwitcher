use anyhow::Context;
use async_trait::async_trait;
use smart_switcher_core::{Module, ModuleContext, ModuleHandle};
use smart_switcher_shared_types::{config::LayoutSwitcherConfig, AppEvent};
use tracing::{debug, info, warn};

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
            let min_autocorrect_len = 5usize;

            info!("✅ layout_switcher запущен");
            info!("   Hotkey: {} (переключение делает Windows)", config.hotkey);
            info!(
                "   Авто-исправление: {}",
                if config.auto_detect { "включено" } else { "выключено" }
            );
            if config.auto_detect {
                info!("   Порог детекта (минимум клавиш): {}", config.detect_threshold);
                info!("   Мин. длина слова для автоисправления: {}", min_autocorrect_len);
            }
            info!("   Для теста: набери 'ghbdtn' + пробел в Блокноте (EN раскладка)");

            let hotkey = config.hotkey.to_lowercase();
            if hotkey != "alt+shift" {
                warn!(hotkey = %config.hotkey, "unsupported hotkey, only alt+shift is supported in MVP");
            }

            let mut is_alt_down = false;
            let mut is_shift_down = false;
            let mut hotkey_fired = false;

            let mut word_keys: Vec<char> = Vec::new();
            let mut word_started_in_cyrillic: Option<bool> = None;

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

            let is_all_upper_ascii = |s: &str| {
                let mut has_letters = false;
                for ch in s.chars() {
                    if ch.is_ascii_alphabetic() {
                        has_letters = true;
                        if !ch.is_ascii_uppercase() {
                            return false;
                        }
                    }
                }
                has_letters
            };

            let is_mixed_case_ascii = |s: &str| {
                let mut has_lower = false;
                let mut has_upper = false;
                for ch in s.chars() {
                    if ch.is_ascii_lowercase() {
                        has_lower = true;
                    } else if ch.is_ascii_uppercase() {
                        has_upper = true;
                    }
                }
                has_lower && has_upper
            };

            let ru_vowel_ratio = |s: &str| {
                let mut vowels = 0usize;
                let mut letters = 0usize;
                for ch in s.chars() {
                    if ch.is_alphabetic() {
                        letters += 1;
                    }
                    if matches!(
                        ch,
                        'а' | 'е' | 'ё' | 'и' | 'о' | 'у' | 'ы' | 'э' | 'ю' | 'я'
                            | 'А' | 'Е' | 'Ё' | 'И' | 'О' | 'У' | 'Ы' | 'Э' | 'Ю' | 'Я'
                    ) {
                        vowels += 1;
                    }
                }
                if letters == 0 {
                    0.0
                } else {
                    vowels as f32 / letters as f32
                }
            };

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
                        info!("⏹️  layout_switcher остановлен");
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
                            // Важно: НЕ выполняем переключение сами.
                            // Иначе при 3+ языках можно получить двойное переключение
                            // (системное + наше) и ощущение "не даёт переключать".
                            info!("⌨️ Alt+Shift: переключение делает Windows");
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
                                if word_keys.is_empty() {
                                    word_started_in_cyrillic = None;
                                }
                            }
                            0x20 => {
                                // Space
                                if word_keys.len() >= config.detect_threshold as usize {
                                    let lang = platform.get_active_lang_id().unwrap_or(0);
                                    let commit_is_cyrillic = platform
                                        .is_active_layout_cyrillic()
                                        .unwrap_or(false);
                                    let commit_is_latin = !commit_is_cyrillic;

                                    // Ключевое: направление определяем по раскладке, в которой НАЧАЛИ слово.
                                    // Это лечит кейс Notepad: набрал в EN, переключил Alt+Shift, нажал пробел.
                                    let word_is_cyrillic = word_started_in_cyrillic.unwrap_or(commit_is_cyrillic);
                                    let word_is_latin = !word_is_cyrillic;

                                    let typed: String = word_keys.iter().collect();

                                    // Важно: мы логируем физические латинские клавиши (VK A-Z).
                                    // Если активна кириллица, то в поле ввода пользователь видит would_be_cyrillic.
                                    let would_be_cyrillic: String =
                                        typed.chars().map(map_en_to_ru).collect();
                                    let screen_guess = if commit_is_cyrillic {
                                        would_be_cyrillic.as_str()
                                    } else {
                                        typed.as_str()
                                    };

                                    let window = platform
                                        .get_foreground_window_info()
                                        .ok()
                                        .unwrap_or_default();

                                    debug!(
                                        word = %typed,
                                        screen_guess = %screen_guess,
                                        window_title = %window.title,
                                        window_process = %window.process_name.unwrap_or_default(),
                                        lang = format_args!("0x{lang:04X}"),
                                        commit_is_latin,
                                        commit_is_cyrillic,
                                        word_is_latin,
                                        word_is_cyrillic,
                                        "space commit"
                                    );

                                    // Консервативный фильтр: не трогаем короткие слова и акронимы.
                                    if typed.len() < min_autocorrect_len
                                        || is_all_upper_ascii(&typed)
                                        || is_mixed_case_ascii(&typed)
                                    {
                                        debug!(
                                            word = %typed,
                                            lang = format_args!("0x{lang:04X}"),
                                            "auto-correct skipped (filter)"
                                        );
                                        word_keys.clear();
                                        continue;
                                    }

                                    if word_is_latin {
                                        // EN (0x0409) -> RU (0x0419)
                                        let converted: String = typed.chars().map(map_en_to_ru).collect();

                                        if !en_vowels(&typed) && ru_vowels(&converted) {
                                            match platform.set_layout_by_lang_id(
                                                &config.forbidden_contexts,
                                                0x0419,
                                            ) {
                                                Ok(true) => debug!("set layout RU: ok"),
                                                Ok(false) => debug!("set layout RU: skipped/failed"),
                                                Err(e) => debug!(error = %e, "set layout RU: error"),
                                            }
                                            // +1 для стирания пробела, который уже попал в поле
                                            let erased = match platform.send_backspaces(
                                                &config.forbidden_contexts,
                                                word_keys.len() + 1,
                                            ) {
                                                Ok(v) => v,
                                                Err(e) => {
                                                    debug!(error = %e, "send_backspaces failed");
                                                    false
                                                }
                                            };
                                            if erased {
                                                // Вставляем исправленный текст + пробел
                                                let text_with_space = format!("{} ", converted);
                                                let injected = match platform.send_unicode_text(
                                                    &config.forbidden_contexts,
                                                    &text_with_space,
                                                ) {
                                                    Ok(v) => v,
                                                    Err(e) => {
                                                        debug!(error = %e, "send_unicode_text failed");
                                                        false
                                                    }
                                                };
                                                if injected {
                                                    info!("🔤 Исправлено EN→RU: '{}' → '{}'", typed, converted);
                                                } else {
                                                    debug!("send_unicode_text returned false");
                                                }
                                            } else {
                                                debug!("send_backspaces returned false");
                                            }
                                        } else {
                                            debug!(
                                                word = %typed,
                                                converted = %converted,
                                                lang = format_args!("0x{lang:04X}"),
                                                "auto-correct skipped (heuristic EN→RU)"
                                            );
                                        }
                                    } else if word_is_cyrillic {
                                        // RU (0x0419) -> EN (0x0409)
                                        // Тут `typed` — это физические латинские клавиши.
                                        // Если пользователь хотел английское слово, оно уже находится в `typed`.
                                        let would_be_ru: String = typed.chars().map(map_en_to_ru).collect();
                                        // Консервативно считаем "похоже на русское" если доля русских гласных высокая.
                                        // Тогда не исправляем. Исправляем только если "как будто RU" выглядит плохо.
                                        if en_vowels(&typed) && ru_vowel_ratio(&would_be_ru) < 0.25 {
                                            match platform.set_layout_by_lang_id(
                                                &config.forbidden_contexts,
                                                0x0409,
                                            ) {
                                                Ok(true) => debug!("set layout EN: ok"),
                                                Ok(false) => debug!("set layout EN: skipped/failed"),
                                                Err(e) => debug!(error = %e, "set layout EN: error"),
                                            }
                                            // +1 для стирания пробела
                                            let erased = match platform.send_backspaces(
                                                &config.forbidden_contexts,
                                                word_keys.len() + 1,
                                            ) {
                                                Ok(v) => v,
                                                Err(e) => {
                                                    debug!(error = %e, "send_backspaces failed");
                                                    false
                                                }
                                            };
                                            if erased {
                                                let text_with_space = format!("{} ", typed);
                                                let injected = match platform.send_unicode_text(
                                                    &config.forbidden_contexts,
                                                    &text_with_space,
                                                ) {
                                                    Ok(v) => v,
                                                    Err(e) => {
                                                        debug!(error = %e, "send_unicode_text failed");
                                                        false
                                                    }
                                                };
                                                if injected {
                                                    info!("🔤 Исправлено RU→EN: набрано в RU раскладке, исправлено на '{}'", typed);
                                                } else {
                                                    debug!("send_unicode_text returned false");
                                                }
                                            } else {
                                                debug!("send_backspaces returned false");
                                            }
                                        } else {
                                            debug!(
                                                word = %typed,
                                                would_be_ru = %would_be_ru,
                                                lang = format_args!("0x{lang:04X}"),
                                                "auto-correct skipped (heuristic RU→EN)"
                                            );
                                        }
                                    } else {
                                        debug!(
                                            word = %typed,
                                            lang = format_args!("0x{lang:04X}"),
                                            "auto-correct skipped (unknown layout class)"
                                        );
                                    }
                                }

                                word_keys.clear();
                                word_started_in_cyrillic = None;
                            }
                            0x0D => {
                                // Enter
                                // Консервативно: НЕ автоисправляем на Enter, чтобы не ломать переносы строк
                                // (в разных приложениях это может быть \n или \r\n).
                                word_keys.clear();
                                word_started_in_cyrillic = None;
                            }
                            vk if is_letter_vk(vk) => {
                                // letters: collect physical key as latin char
                                if word_keys.is_empty() {
                                    word_started_in_cyrillic = Some(
                                        platform.is_active_layout_cyrillic().unwrap_or(false),
                                    );
                                }
                                let ch = vk_to_letter(vk, is_shift_down);
                                word_keys.push(ch);
                            }
                            _ => {
                                // delimiter / control
                                word_keys.clear();
                                word_started_in_cyrillic = None;
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
