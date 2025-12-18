use anyhow::Context;
use async_trait::async_trait;
use smart_switcher_core::{Module, ModuleContext, ModuleHandle};
use smart_switcher_shared_types::{config::SpellCheckerConfig, AppEvent};
use tracing::info;

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

        let join = tokio::spawn(async move {
            info!(
                enabled = config.enabled,
                api = %config.api,
                language = %config.language,
                cache_size = config.cache_size,
                base_url = %config.api_config.base_url,
                "spell_checker started",
            );

            loop {
                match rx.recv().await.context("event bus recv")? {
                    AppEvent::ShutdownRequested => {
                        info!("spell_checker shutting down");
                        break;
                    }
                }
            }

            Ok(())
        });

        Ok(ModuleHandle::new(join))
    }
}
