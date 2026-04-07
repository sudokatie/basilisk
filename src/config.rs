//! Configuration parsing
//!
//! TOML-based configuration for fonts, colors, keybindings, and window settings.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::term::cell::{Color, ColorPalette};

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
    pub bold_italic_font: Option<String>,
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
            bold_italic_font: None,
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
    /// Convert color scheme to a terminal color palette
    pub fn to_palette(&self) -> ColorPalette {
        let parse = |hex: &str| -> Color {
            Color::from_hex(hex).unwrap_or(Color::rgb(255, 255, 255))
        };

        ColorPalette::from_config(
            parse(&self.black),
            parse(&self.red),
            parse(&self.green),
            parse(&self.yellow),
            parse(&self.blue),
            parse(&self.magenta),
            parse(&self.cyan),
            parse(&self.white),
            parse(&self.bright_black),
            parse(&self.bright_red),
            parse(&self.bright_green),
            parse(&self.bright_yellow),
            parse(&self.bright_blue),
            parse(&self.bright_magenta),
            parse(&self.bright_cyan),
            parse(&self.bright_white),
        )
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

    /// Reload config from disk, returning only if changed
    pub fn reload(path: &Path, current: &Self) -> Option<Self> {
        match Self::load(path) {
            Ok(new_config) => {
                // Check if anything relevant changed
                if new_config.colors != current.colors
                    || new_config.font != current.font
                    || new_config.window.opacity != current.window.opacity
                {
                    Some(new_config)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }
}

// Implement PartialEq for ColorScheme to detect changes
impl PartialEq for ColorScheme {
    fn eq(&self, other: &Self) -> bool {
        self.foreground == other.foreground
            && self.background == other.background
            && self.cursor == other.cursor
            && self.black == other.black
            && self.red == other.red
            && self.green == other.green
            && self.yellow == other.yellow
            && self.blue == other.blue
            && self.magenta == other.magenta
            && self.cyan == other.cyan
            && self.white == other.white
    }
}

impl PartialEq for FontConfig {
    fn eq(&self, other: &Self) -> bool {
        self.family == other.family
            && (self.size - other.size).abs() < 0.01
            && self.bold_font == other.bold_font
            && self.italic_font == other.italic_font
            && self.bold_italic_font == other.bold_italic_font
    }
}

use std::sync::mpsc;
use notify::{Watcher, RecursiveMode, Event, EventKind};

/// Config file watcher for hot reload
pub struct ConfigWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: mpsc::Receiver<std::result::Result<Event, notify::Error>>,
    config_path: PathBuf,
}

impl ConfigWatcher {
    /// Create a new config watcher for the given path
    pub fn new(config_path: PathBuf) -> Result<Self> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }).map_err(|e| crate::Error::Config(e.to_string()))?;

        // Watch the config file's parent directory to catch recreations
        if let Some(parent) = config_path.parent() {
            watcher.watch(parent, RecursiveMode::NonRecursive)
                .map_err(|e| crate::Error::Config(e.to_string()))?;
        }

        Ok(Self {
            _watcher: watcher,
            rx,
            config_path,
        })
    }

    /// Check if config file was modified (non-blocking)
    pub fn check_modified(&self) -> bool {
        while let Ok(Ok(event)) = self.rx.try_recv() {
            // Check if the event is for our config file
            if let EventKind::Modify(_) | EventKind::Create(_) = event.kind {
                for path in event.paths {
                    if path == self.config_path {
                        return true;
                    }
                }
            }
        }
        false
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

    #[test]
    fn color_scheme_to_palette() {
        let scheme = ColorScheme::default();
        let palette = scheme.to_palette();
        
        // Check that colors are properly converted from hex
        // Default black is "#282a2e"
        let black = palette.get(0);
        assert_eq!(black.r, 0x28);
        assert_eq!(black.g, 0x2a);
        assert_eq!(black.b, 0x2e);
        
        // Default red is "#a54242"
        let red = palette.get(1);
        assert_eq!(red.r, 0xa5);
        assert_eq!(red.g, 0x42);
        assert_eq!(red.b, 0x42);
    }

    #[test]
    fn custom_color_scheme_to_palette() {
        let mut scheme = ColorScheme::default();
        scheme.red = "#ff0000".into();
        scheme.bright_red = "#ff8080".into();
        
        let palette = scheme.to_palette();
        
        // Check custom red
        let red = palette.get(1);
        assert_eq!(red.r, 255);
        assert_eq!(red.g, 0);
        assert_eq!(red.b, 0);
        
        // Check custom bright red
        let bright_red = palette.get(9);
        assert_eq!(bright_red.r, 255);
        assert_eq!(bright_red.g, 0x80);
        assert_eq!(bright_red.b, 0x80);
    }
}
