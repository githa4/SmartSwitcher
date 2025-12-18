use anyhow::Context;
use async_trait::async_trait;
use smart_switcher_core::{Module, ModuleContext, ModuleHandle};
use smart_switcher_shared_types::{config::LayoutSwitcherConfig, AppEvent};
use tracing::{debug, info};

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

        let join = tokio::spawn(async move {
            info!(
                enabled = config.enabled,
                auto_detect = config.auto_detect,
                detect_threshold = config.detect_threshold,
                hotkey = %config.hotkey,
                "layout_switcher started",
            );

            loop {
                match rx.recv().await.context("event bus recv")? {
                    AppEvent::ShutdownRequested => {
                        info!("layout_switcher shutting down");
                        break;
                    }
                    AppEvent::Keyboard(ev) => {
                        if ev.is_key_down {
                            debug!(vk_code = ev.vk_code, "key down");
                        }
                    }
                }
            }

            Ok(())
        });

        Ok(ModuleHandle::new(join))
    }
}
