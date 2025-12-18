use std::path::PathBuf;

use anyhow::Context;
use layout_switcher::LayoutSwitcherModule;
use smart_switcher_core::{is_module_loaded, load_config, Module, ModuleContext, Runtime};
use smart_switcher_shared_types::AppEvent;
use spell_checker::SpellCheckerModule;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

fn init_tracing(level: &str, output: &str) {
    if output != "console" {
        warn!(output = %output, "logging output is not supported yet, using console");
    }

    let env_filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = PathBuf::from("config.toml");
    let config = load_config(&config_path).context("load config")?;

    init_tracing(&config.logging.level, &config.logging.output);
    info!("smart_switcher starting");

    let runtime = Runtime::new(config_path, config);
    let ctx = ModuleContext {
        bus: runtime.bus.clone(),
        platform: runtime.platform.clone(),
    };

    #[cfg(target_os = "windows")]
    let (mut keyboard_hook_controller, mut keyboard_forward_join) = {
        let should_start_hook = runtime.config.layout_switcher.enabled
            && is_module_loaded(&runtime.config, "layout_switcher");

        if should_start_hook {
            let hook = runtime
                .platform
                .start_keyboard_hook()
                .context("start keyboard hook")?;
            let (controller, events_rx) = hook.into_parts();

            let bus = runtime.bus.clone();
            let forward = std::thread::spawn(move || {
                for ev in events_rx {
                    bus.send(AppEvent::Keyboard(ev));
                }
            });

            (Some(controller), Some(forward))
        } else {
            (None, None)
        }
    };

    let mut handles = Vec::new();
    let modules: Vec<Box<dyn Module>> = vec![
        Box::new(LayoutSwitcherModule::new(
            runtime.config.layout_switcher.clone(),
        )),
        Box::new(SpellCheckerModule::new(
            runtime.config.spell_checker.clone(),
        )),
    ];

    for module in modules {
        let name = module.name();
        let enabled = match name {
            "layout_switcher" => runtime.config.layout_switcher.enabled,
            "spell_checker" => runtime.config.spell_checker.enabled,
            _ => false,
        };

        if !is_module_loaded(&runtime.config, name) {
            info!(module = name, "module not loaded");
            continue;
        }

        if !enabled {
            info!(module = name, "module loaded but disabled");
            continue;
        }

        info!(module = name, "starting module");
        handles.push(module.start(ctx.clone()).await?);
    }

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl+C received");
        }
    }

    runtime.bus.send(AppEvent::ShutdownRequested);
    for handle in handles {
        handle.join().await?;
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(controller) = keyboard_hook_controller.take() {
            controller.stop();
        }
        if let Some(forward) = keyboard_forward_join.take() {
            let _ = forward.join();
        }
    }

    info!("smart_switcher stopped");
    Ok(())
}
