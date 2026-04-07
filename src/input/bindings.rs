//! Key bindings configuration

use std::collections::HashMap;

/// Actions that can be bound to keys
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    NewWindow,
    ClosePane,
    SplitHorizontal,
    SplitVertical,
    NextWindow,
    PrevWindow,
    Copy,
    Paste,
    Detach,
    ZoomPane,
    CopyMode,
    Search,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollToTop,
    ScrollToBottom,
    FocusPaneUp,
    FocusPaneDown,
    FocusPaneLeft,
    FocusPaneRight,
    SelectWindow(usize),
}

/// Parsed key combination
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: char,
}

impl KeyCombo {
    /// Parse a key combo string like "ctrl+shift+c"
    pub fn parse(s: &str) -> Option<Self> {
        let lowered = s.to_lowercase();
        let parts: Vec<&str> = lowered.split('+').collect();
        if parts.is_empty() {
            return None;
        }

        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key = None;

        for part in parts {
            match part.trim() {
                "ctrl" | "control" => ctrl = true,
                "alt" | "meta" | "option" => alt = true,
                "shift" => shift = true,
                k if k.len() == 1 => key = k.chars().next(),
                "space" => key = Some(' '),
                "enter" | "return" => key = Some('\n'),
                "tab" => key = Some('\t'),
                "escape" | "esc" => key = Some('\x1b'),
                _ => {}
            }
        }

        key.map(|k| KeyCombo { ctrl, alt, shift, key: k })
    }
}

/// Key bindings manager
pub struct Bindings {
    /// Map from key combo to action
    bindings: HashMap<KeyCombo, Action>,
}

impl Bindings {
    /// Create empty bindings
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Create default bindings (non-prefix keys like Ctrl+Shift+C for copy)
    pub fn default_bindings() -> Self {
        let mut bindings = HashMap::new();

        // Default non-prefix bindings
        bindings.insert(
            KeyCombo { ctrl: true, alt: false, shift: true, key: 'c' },
            Action::Copy,
        );
        bindings.insert(
            KeyCombo { ctrl: true, alt: false, shift: true, key: 'v' },
            Action::Paste,
        );
        bindings.insert(
            KeyCombo { ctrl: true, alt: false, shift: true, key: 'f' },
            Action::Search,
        );

        Self { bindings }
    }

    /// Load bindings from config hashmap
    pub fn from_config(config: &HashMap<String, String>) -> Self {
        let mut bindings = Self::default_bindings();

        for (action_name, key_combo) in config {
            if let Some(combo) = KeyCombo::parse(key_combo) {
                if let Some(action) = parse_action(action_name) {
                    bindings.bindings.insert(combo, action);
                }
            }
        }

        bindings
    }

    /// Bind a key combo to an action
    pub fn bind(&mut self, combo: KeyCombo, action: Action) {
        self.bindings.insert(combo, action);
    }

    /// Look up action for a key combo
    pub fn get(&self, combo: &KeyCombo) -> Option<&Action> {
        self.bindings.get(combo)
    }

    /// Check if a key combo is bound
    pub fn is_bound(&self, combo: &KeyCombo) -> bool {
        self.bindings.contains_key(combo)
    }
}

impl Default for Bindings {
    fn default() -> Self {
        Self::default_bindings()
    }
}

/// Parse action name to Action enum
fn parse_action(name: &str) -> Option<Action> {
    match name.to_lowercase().as_str() {
        "new_window" | "newwindow" => Some(Action::NewWindow),
        "close_pane" | "closepane" => Some(Action::ClosePane),
        "split_horizontal" | "splithorizontal" | "hsplit" => Some(Action::SplitHorizontal),
        "split_vertical" | "splitvertical" | "vsplit" => Some(Action::SplitVertical),
        "next_window" | "nextwindow" => Some(Action::NextWindow),
        "prev_window" | "prevwindow" => Some(Action::PrevWindow),
        "copy" => Some(Action::Copy),
        "paste" => Some(Action::Paste),
        "detach" => Some(Action::Detach),
        "zoom_pane" | "zoompane" | "zoom" => Some(Action::ZoomPane),
        "copy_mode" | "copymode" => Some(Action::CopyMode),
        "search" => Some(Action::Search),
        "scroll_up" | "scrollup" => Some(Action::ScrollUp),
        "scroll_down" | "scrolldown" => Some(Action::ScrollDown),
        "scroll_page_up" | "scrollpageup" | "pageup" => Some(Action::ScrollPageUp),
        "scroll_page_down" | "scrollpagedown" | "pagedown" => Some(Action::ScrollPageDown),
        "scroll_to_top" | "scrolltotop" => Some(Action::ScrollToTop),
        "scroll_to_bottom" | "scrolltobottom" => Some(Action::ScrollToBottom),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_combo() {
        let combo = KeyCombo::parse("ctrl+shift+c").unwrap();
        assert!(combo.ctrl);
        assert!(combo.shift);
        assert!(!combo.alt);
        assert_eq!(combo.key, 'c');
    }

    #[test]
    fn parse_simple_key() {
        let combo = KeyCombo::parse("a").unwrap();
        assert!(!combo.ctrl);
        assert!(!combo.shift);
        assert!(!combo.alt);
        assert_eq!(combo.key, 'a');
    }

    #[test]
    fn default_bindings_copy() {
        let bindings = Bindings::default_bindings();
        let combo = KeyCombo { ctrl: true, alt: false, shift: true, key: 'c' };
        assert_eq!(bindings.get(&combo), Some(&Action::Copy));
    }
}
