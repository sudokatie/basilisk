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
    pub terminal: TerminalConfig,
    pub keybinds: KeybindsConfig,
}

/// Window decoration style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Decorations {
    #[default]
    Full,
    None,
    Transparent,
}

impl<'de> Deserialize<'de> for Decorations {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "full" => Ok(Decorations::Full),
            "none" => Ok(Decorations::None),
            "transparent" => Ok(Decorations::Transparent),
            _ => Ok(Decorations::Full),
        }
    }
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
    pub width: u32,
    pub height: u32,
    pub decorations: String,
    pub opacity: f32,
    pub padding: u32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct TerminalConfig {
    pub shell: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct KeybindsConfig {
    /// Prefix key for multiplexer commands (e.g., "ctrl+b")
    pub prefix: String,
    /// Custom keybindings (action -> key)
    #[serde(flatten)]
    pub custom: std::collections::HashMap<String, String>,
}

impl Default for KeybindsConfig {
    fn default() -> Self {
        Self {
            prefix: "ctrl+b".into(),
            custom: std::collections::HashMap::new(),
        }
    }
}

impl KeybindsConfig {
    /// Parse the prefix key into modifier and key
    pub fn parse_prefix(&self) -> Option<(bool, bool, bool, char)> {
        // Returns (ctrl, alt, shift, key)
        let parts: Vec<&str> = self.prefix.split('+').collect();
        if parts.is_empty() {
            return None;
        }

        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key = None;

        for part in parts {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" | "meta" | "option" => alt = true,
                "shift" => shift = true,
                s if s.len() == 1 => key = s.chars().next(),
                _ => {}
            }
        }

        key.map(|k| (ctrl, alt, shift, k))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font: FontConfig::default(),
            colors: ColorScheme::default(),
            scrollback: ScrollbackConfig::default(),
            window: WindowConfig::default(),
            terminal: TerminalConfig::default(),
            keybinds: KeybindsConfig::default(),
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

impl ColorScheme {
    /// Parse a hex color string to RGB
    fn parse_hex(&self, hex: &str) -> (u8, u8, u8) {
        let hex = hex.trim_start_matches('#');
        if hex.len() >= 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
            (r, g, b)
        } else {
            (255, 255, 255)
        }
    }

    /// Build a 256-color palette with the first 16 colors from config
    pub fn build_palette(&self) -> Vec<crate::term::cell::Color> {
        use crate::term::cell::Color;
        
        let mut palette = Vec::with_capacity(256);
        
        // Standard 16 colors from config
        let colors = [
            &self.black,
            &self.red,
            &self.green,
            &self.yellow,
            &self.blue,
            &self.magenta,
            &self.cyan,
            &self.white,
            &self.bright_black,
            &self.bright_red,
            &self.bright_green,
            &self.bright_yellow,
            &self.bright_blue,
            &self.bright_magenta,
            &self.bright_cyan,
            &self.bright_white,
        ];

        for hex in colors {
            let (r, g, b) = self.parse_hex(hex);
            palette.push(Color::rgb(r, g, b));
        }

        // 216 color cube (16-231)
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    let rv = if r == 0 { 0 } else { 55 + r * 40 };
                    let gv = if g == 0 { 0 } else { 55 + g * 40 };
                    let bv = if b == 0 { 0 } else { 55 + b * 40 };
                    palette.push(Color::rgb(rv, gv, bv));
                }
            }
        }

        // 24 grayscale colors (232-255)
        for i in 0..24 {
            let gray = 8 + i * 10;
            palette.push(Color::rgb(gray, gray, gray));
        }

        palette
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
            width: 800,
            height: 600,
            decorations: "full".into(),
            opacity: 1.0,
            padding: 2,
        }
    }
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            cols: 80,
            rows: 24,
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
