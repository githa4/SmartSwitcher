use anyhow::Context;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use smart_switcher_core::{Module, ModuleContext, ModuleHandle};
use smart_switcher_shared_types::{config::SpellCheckerConfig, AppEvent};
use tracing::{info, warn};

pub struct SpellCheckerModule {
    config: SpellCheckerConfig,
}

impl SpellCheckerModule {
    pub fn new(config: SpellCheckerConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Module for SpellCheckerModule {
    fn name(&self) -> &'static str {
        "spell_checker"
    }

    async fn start(&self, ctx: ModuleContext) -> anyhow::Result<ModuleHandle> {
        let mut rx = ctx.bus.subscribe();
        let config = self.config.clone();
        let platform = ctx.platform.clone();

        let client = Client::builder()
            .user_agent("smart_switcher/0.1")
            .build()
            .context("build http client")?;

        let join = tokio::spawn(async move {
            info!(
                enabled = config.enabled,
                api = %config.api,
                language = %config.language,
                cache_size = config.cache_size,
                base_url = %config.api_config.base_url,
                "spell_checker started",
            );

            let is_letter_vk = |vk: u32| (0x41..=0x5A).contains(&vk);
            let vk_to_letter = |vk: u32, shift: bool| {
                let base = (vk as u8 as char).to_ascii_lowercase();
                if shift {
                    base.to_ascii_uppercase()
                } else {
                    base
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

            let mut is_alt_down = false;
            let mut is_shift_down = false;
            let mut buffer = String::new();

            loop {
                match rx.recv().await.context("event bus recv")? {
                    AppEvent::ShutdownRequested => {
                        info!("spell_checker shutting down");
                        break;
                    }
                    AppEvent::Keyboard(ev) => {
                        if is_alt_vk(ev.vk_code) {
                            is_alt_down = ev.is_key_down;
                        }
                        if is_shift_vk(ev.vk_code) {
                            is_shift_down = ev.is_key_down;
                        }

                        if !ev.is_key_down {
                            continue;
                        }

                        if is_alt_down {
                            continue;
                        }

                        match ev.vk_code {
                            0x08 => {
                                // Backspace
                                buffer.pop();
                            }
                            0x20 => {
                                // Space
                                if !buffer.ends_with(' ') {
                                    buffer.push(' ');
                                }
                            }
                            0x0D => {
                                // Enter => commit
                                let commit = buffer.trim().to_string();
                                buffer.clear();

                                if commit.is_empty() {
                                    continue;
                                }

                                let forbidden = platform
                                    .is_forbidden_context(&config.forbidden_contexts)
                                    .unwrap_or(true);
                                if forbidden {
                                    continue;
                                }

                                if config.api.to_lowercase() != "languagetool" {
                                    warn!(api = %config.api, "unsupported spell_checker api (only languagetool is supported in MVP)");
                                    continue;
                                }

                                match languagetool_check(&client, &config, &commit).await {
                                    Ok(result) => {
                                        if result.matches.is_empty() {
                                            info!("spell_checker: no issues");
                                        } else {
                                            let count = result.matches.len();
                                            let first = &result.matches[0];
                                            warn!(
                                                issues = count,
                                                message = %first.message,
                                                "spell_checker: issues found"
                                            );
                                        }
                                    }
                                    Err(err) => {
                                        warn!(error = %err, "spell_checker request failed");
                                    }
                                }
                            }
                            vk if is_letter_vk(vk) => {
                                let base = vk_to_letter(vk, is_shift_down);
                                let lang = platform.get_active_lang_id().unwrap_or(0);

                                let ch = if lang == 0x0419 {
                                    // RU
                                    let ru = map_en_to_ru(base);
                                    if is_shift_down {
                                        ru.to_uppercase().next().unwrap_or(ru)
                                    } else {
                                        ru
                                    }
                                } else {
                                    // EN or unknown
                                    base
                                };

                                buffer.push(ch);
                            }
                            _ => {}
                        }
                    }
                }
            }

            Ok(())
        });

        Ok(ModuleHandle::new(join))
    }
}

#[derive(Debug, Deserialize)]
struct LanguageToolResponse {
    #[serde(default)]
    matches: Vec<LanguageToolMatch>,
}

#[derive(Debug, Deserialize)]
struct LanguageToolMatch {
    #[serde(default)]
    message: String,
}

async fn languagetool_check(
    client: &Client,
    config: &SpellCheckerConfig,
    text: &str,
) -> anyhow::Result<LanguageToolResponse> {
    let base = config.api_config.base_url.trim_end_matches('/');
    let url = format!("{base}/v2/check");

    let res = client
        .post(url)
        .form(&[("text", text), ("language", config.language.as_str())])
        .send()
        .await
        .context("send request")?
        .error_for_status()
        .context("non-success status")?;

    res.json::<LanguageToolResponse>()
        .await
        .context("parse response")
}
