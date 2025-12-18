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
                    }
                }
            }

            Ok(())
        });

        Ok(ModuleHandle::new(join))
    }
}
