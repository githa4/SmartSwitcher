use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::Context;
use async_trait::async_trait;
use smart_switcher_platform::Platform;
use smart_switcher_shared_types::{AppEvent, Config};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AppEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.sender.subscribe()
    }

    pub fn send(&self, event: AppEvent) {
        let _ = self.sender.send(event);
    }
}

#[derive(Clone)]
pub struct ModuleContext {
    pub bus: EventBus,
    pub platform: Platform,
}

#[derive(Debug)]
pub struct ModuleHandle {
    join: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl ModuleHandle {
    pub fn new(join: tokio::task::JoinHandle<anyhow::Result<()>>) -> Self {
        Self { join }
    }

    pub async fn join(self) -> anyhow::Result<()> {
        self.join
            .await
            .context("module task panicked")?
            .context("module task returned error")
    }
}

#[async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &'static str;
    async fn start(&self, ctx: ModuleContext) -> anyhow::Result<ModuleHandle>;
}

pub fn load_config(path: impl AsRef<Path>) -> anyhow::Result<Config> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config: {}", path.display()))?;
    toml::from_str(&raw).context("failed to parse config.toml")
}

pub fn is_module_loaded(config: &Config, name: &str) -> bool {
    let loaded: HashSet<&str> = config.modules.loaded.iter().map(|s| s.as_str()).collect();
    let disabled: HashSet<&str> =
        config.modules.disabled.iter().map(|s| s.as_str()).collect();
    loaded.contains(name) && !disabled.contains(name)
}

pub struct Runtime {
    pub config_path: PathBuf,
    pub config: Config,
    pub bus: EventBus,
    pub platform: Platform,
}

impl Runtime {
    pub fn new(config_path: PathBuf, config: Config) -> Self {
        Self {
            config_path,
            config,
            bus: EventBus::new(256),
            platform: Platform::new(),
        }
    }
}
