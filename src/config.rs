//! Configuration parsing
//!
//! TOML-based configuration for fonts, colors, keybindings, and window settings.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::Result;

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub font: FontConfig,
    pub colors: ColorScheme,
    pub scrollback: ScrollbackConfig,
    pub window: WindowConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
    pub bold_font: Option<String>,
    pub italic_font: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct ColorScheme {
    pub foreground: String,
    pub background: String,
    pub cursor: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct ScrollbackConfig {
    pub lines: usize,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct WindowConfig {
    pub decorations: String,
    pub opacity: f32,
    pub padding: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font: FontConfig::default(),
            colors: ColorScheme::default(),
            scrollback: ScrollbackConfig::default(),
            window: WindowConfig::default(),
        }
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "monospace".into(),
            size: 14.0,
            bold_font: None,
            italic_font: None,
        }
    }
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            foreground: "#c5c8c6".into(),
            background: "#1d1f21".into(),
            cursor: "#c5c8c6".into(),
            black: "#282a2e".into(),
            red: "#a54242".into(),
            green: "#8c9440".into(),
            yellow: "#de935f".into(),
            blue: "#5f819d".into(),
            magenta: "#85678f".into(),
            cyan: "#5e8d87".into(),
            white: "#707880".into(),
            bright_black: "#373b41".into(),
            bright_red: "#cc6666".into(),
            bright_green: "#b5bd68".into(),
            bright_yellow: "#f0c674".into(),
            bright_blue: "#81a2be".into(),
            bright_magenta: "#b294bb".into(),
            bright_cyan: "#8abeb7".into(),
            bright_white: "#c5c8c6".into(),
        }
    }
}

impl Default for ScrollbackConfig {
    fn default() -> Self {
        Self { lines: 10000 }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            decorations: "full".into(),
            opacity: 1.0,
            padding: 2,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| crate::Error::Config(e.to_string()))
    }

    pub fn default_path() -> PathBuf {
        dirs_path().join("config.toml")
    }
}

fn dirs_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("basilisk")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = Config::default();
        assert_eq!(config.font.size, 14.0);
        assert_eq!(config.scrollback.lines, 10000);
    }

    #[test]
    fn parse_config() {
        let toml = r#"
[font]
family = "JetBrains Mono"
size = 16

[scrollback]
lines = 5000
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.font.family, "JetBrains Mono");
        assert_eq!(config.font.size, 16.0);
        assert_eq!(config.scrollback.lines, 5000);
    }
}
