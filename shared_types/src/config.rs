use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Config {
    pub logging: LoggingConfig,
    pub layout_switcher: LayoutSwitcherConfig,
    pub spell_checker: SpellCheckerConfig,
    pub modules: ModulesConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            logging: LoggingConfig::default(),
            layout_switcher: LayoutSwitcherConfig::default(),
            spell_checker: SpellCheckerConfig::default(),
            modules: ModulesConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct LoggingConfig {
    pub level: String,
    pub output: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            output: "console".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct LayoutSwitcherConfig {
    pub enabled: bool,
    pub hotkey: String,
    pub auto_detect: bool,
    pub detect_threshold: u8,
    pub forbidden_contexts: ForbiddenContextsConfig,
}

impl Default for LayoutSwitcherConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            hotkey: "alt+shift".to_string(),
            auto_detect: true,
            detect_threshold: 3,
            forbidden_contexts: ForbiddenContextsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct ForbiddenContextsConfig {
    pub blocked_processes: Vec<String>,
    pub blocked_windows: Vec<String>,
    pub blocked_input_types: Vec<String>,
}

impl Default for ForbiddenContextsConfig {
    fn default() -> Self {
        Self {
            blocked_processes: Vec::new(),
            blocked_windows: Vec::new(),
            blocked_input_types: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct SpellCheckerConfig {
    pub enabled: bool,
    pub api: String,
    pub language: String,
    pub cache_size: usize,
    pub api_config: SpellCheckerApiConfig,
}

impl Default for SpellCheckerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api: "languagetool".to_string(),
            language: "ru".to_string(),
            cache_size: 1000,
            api_config: SpellCheckerApiConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct SpellCheckerApiConfig {
    pub base_url: String,
}

impl Default for SpellCheckerApiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.languagetool.org".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct ModulesConfig {
    pub loaded: Vec<String>,
    pub disabled: Vec<String>,
}

impl Default for ModulesConfig {
    fn default() -> Self {
        Self {
            loaded: vec![
                "layout_switcher".to_string(),
                "spell_checker".to_string(),
            ],
            disabled: Vec::new(),
        }
    }
}
