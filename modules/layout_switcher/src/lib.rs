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
            info!("   Для теста: набери 'ghbdtn' + пробел в любом поле ввода (EN раскладка)");

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

            let map_en_to_ru = |ch: char| -> char { map_en_to_ru(ch) };

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
                            }
                            0x20 => {
                                // Space
                                if word_keys.len() >= config.detect_threshold as usize {
                                    // Fail-closed: никаких действий в запрещённых контекстах.
                                    // Сразу выходим, чтобы не "подвешивать" эвристики в терминалах/менеджерах паролей.
                                    match platform.is_forbidden_context(&config.forbidden_contexts) {
                                        Ok(true) => {
                                            debug!("auto-correct skipped (forbidden context)");
                                            word_keys.clear();
                                            continue;
                                        }
                                        Ok(false) => {}
                                        Err(e) => {
                                            debug!(error = %e, "auto-correct skipped (forbidden context check failed)");
                                            word_keys.clear();
                                            continue;
                                        }
                                    }

                                    let lang = platform.get_active_lang_id().unwrap_or(0);
                                    let commit_is_cyrillic = is_cyrillic_lang_id(lang);
                                    let commit_is_latin = !commit_is_cyrillic;

                                    let typed: String = word_keys.iter().collect();

                                    debug!(
                                        word = %typed,
                                        lang = format_args!("0x{lang:04X}"),
                                        commit_is_latin,
                                        commit_is_cyrillic,
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

                                    if commit_is_latin {
                                        // EN (0x0409) -> RU (0x0419)
                                        let converted: String = typed.chars().map(map_en_to_ru).collect();

                                        if should_autocorrect_en_to_ru(&typed, &converted) {
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
                                    } else if commit_is_cyrillic {
                                        // RU (0x0419) -> EN (0x0409)
                                        // Тут `typed` — это физические латинские клавиши.
                                        // Если пользователь хотел английское слово, оно уже находится в `typed`.
                                        let would_be_ru: String = typed.chars().map(map_en_to_ru).collect();

                                        // Если то, что видно на экране, выглядит как нормальное русское слово — не трогаем.
                                        // Исправляем только когда "экранное RU" выглядит как мусор, а `typed` похоже на EN.
                                        if should_autocorrect_ru_to_en(&typed, &would_be_ru) {
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
                            }
                            0x0D => {
                                // Enter
                                // Консервативно: НЕ автоисправляем на Enter, чтобы не ломать переносы строк
                                // (в разных приложениях это может быть \n или \r\n).
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

fn primary_lang_id(lang_id: u16) -> u16 {
    lang_id & 0x03FF
}

fn is_cyrillic_lang_id(lang_id: u16) -> bool {
    matches!(primary_lang_id(lang_id), 0x0019 | 0x0022 | 0x0023)
}

fn is_ascii_word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic())
}

fn map_en_to_ru(ch: char) -> char {
    match ch.to_ascii_lowercase() {
        'q' => 'й',
        'w' => 'ц',
        'e' => 'у',
        'r' => 'к',
        't' => 'е',
        'y' => 'н',
        'u' => 'г',
        'i' => 'ш',
        'o' => 'щ',
        'p' => 'з',
        'a' => 'ф',
        's' => 'ы',
        'd' => 'в',
        'f' => 'а',
        'g' => 'п',
        'h' => 'р',
        'j' => 'о',
        'k' => 'л',
        'l' => 'д',
        'z' => 'я',
        'x' => 'ч',
        'c' => 'с',
        'v' => 'м',
        'b' => 'и',
        'n' => 'т',
        'm' => 'ь',
        other => other,
    }
}

fn en_vowel_ratio(s: &str) -> f32 {
    let mut vowels = 0usize;
    let mut letters = 0usize;

    for ch in s.chars() {
        if ch.is_ascii_alphabetic() {
            letters += 1;
            if matches!(
                ch,
                'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'A' | 'E' | 'I' | 'O' | 'U' | 'Y'
            ) {
                vowels += 1;
            }
        }
    }

    if letters == 0 {
        0.0
    } else {
        vowels as f32 / letters as f32
    }
}

fn ru_vowel_ratio(s: &str) -> f32 {
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
}

fn looks_like_english_word(typed: &str) -> bool {
    if !is_ascii_word(typed) {
        return false;
    }

    let ratio = en_vowel_ratio(typed);
    if ratio < 0.15 || ratio > 0.70 {
        return false;
    }

    // Небольшой бонус к уверенности: частые EN биграммы.
    let lower = typed.to_ascii_lowercase();
    ["th", "sh", "ch", "ck", "qu", "ng", "oo", "ee"]
        .iter()
        .any(|b| lower.contains(b))
        || ratio >= 0.25
}

fn has_strong_english_bigrams(typed: &str) -> bool {
    let lower = typed.to_ascii_lowercase();
    ["th", "sh", "ch", "ck", "qu", "ng", "oo", "ee"]
        .iter()
        .any(|b| lower.contains(b))
}

fn should_autocorrect_en_to_ru(typed: &str, converted: &str) -> bool {
    if !is_ascii_word(typed) {
        return false;
    }
    if looks_like_english_word(typed) {
        return false;
    }

    // Если в русском варианте есть "нормальная" гласность — это хороший сигнал,
    // что пользователь хотел русское слово.
    ru_vowel_ratio(converted) >= 0.20
}

fn should_autocorrect_ru_to_en(typed: &str, would_be_ru: &str) -> bool {
    if !is_ascii_word(typed) {
        return false;
    }
    if !looks_like_english_word(typed) {
        return false;
    }

    // Если "экранное" RU похоже на реальное русское слово — не трогаем.
    // Исправляем только когда оно выглядит как мусор. Для высокой уверенности ("th", "sh"...)
    // допускаем более мягкий порог, чтобы ловить кейсы вроде "thanks" → "ерфтлы".
    let ru_ratio = ru_vowel_ratio(would_be_ru);
    if ru_ratio < 0.25 {
        return true;
    }

    has_strong_english_bigrams(typed) && ru_ratio < 0.45
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primary_lang_id() {
        assert_eq!(primary_lang_id(0x0419), 0x0019);
        assert_eq!(primary_lang_id(0x0422), 0x0022);
        assert_eq!(primary_lang_id(0x0423), 0x0023);
    }

    #[test]
    fn test_lang_classification() {
        assert!(is_cyrillic_lang_id(0x0419));
        assert!(is_cyrillic_lang_id(0x0422));
        assert!(is_cyrillic_lang_id(0x0423));
        assert!(!is_cyrillic_lang_id(0x0409));
    }

    #[test]
    fn test_map_en_to_ru_basic() {
        let typed = "ghbdtn";
        let converted: String = typed.chars().map(map_en_to_ru).collect();
        assert_eq!(converted, "привет");
    }

    #[test]
    fn test_should_autocorrect_en_to_ru() {
        let typed = "ghbdtn";
        let converted: String = typed.chars().map(map_en_to_ru).collect();
        assert!(should_autocorrect_en_to_ru(typed, &converted));

        let typed = "hello";
        let converted: String = typed.chars().map(map_en_to_ru).collect();
        assert!(!should_autocorrect_en_to_ru(typed, &converted));
    }

    #[test]
    fn test_should_autocorrect_ru_to_en() {
        // Пользователь в RU раскладке хотел EN: 'hello' на экране выглядит как 'руддщ'.
        let typed = "hello";
        let would_be_ru: String = typed.chars().map(map_en_to_ru).collect();
        assert!(should_autocorrect_ru_to_en(typed, &would_be_ru));

        // Типовой кейс: в RU раскладке хотел EN, а на экране получилось "похоже на слово",
        // но это всё равно мусор для пользователя.
        let typed = "thanks";
        let would_be_ru: String = typed.chars().map(map_en_to_ru).collect();
        assert!(should_autocorrect_ru_to_en(typed, &would_be_ru));

        // Пользователь реально набирал русское: на экране это похоже на слово.
        let typed = "ghbdtn";
        let would_be_ru: String = typed.chars().map(map_en_to_ru).collect();
        assert!(!should_autocorrect_ru_to_en(typed, &would_be_ru));
    }
}
