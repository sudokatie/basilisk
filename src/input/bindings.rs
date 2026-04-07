//! Key bindings for multiplexer commands
//!
//! Handles prefix-key based bindings (tmux-style) for window/pane management.

use std::collections::HashMap;

/// Actions that can be triggered by keybindings
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// Create a new window
    NewWindow,
    /// Close the current pane
    ClosePane,
    /// Split pane horizontally (top/bottom)
    SplitHorizontal,
    /// Split pane vertically (left/right)
    SplitVertical,
    /// Focus next window
    NextWindow,
    /// Focus previous window
    PrevWindow,
    /// Select window by number (0-9)
    SelectWindow(u8),
    /// Focus next pane
    NextPane,
    /// Focus previous pane
    PrevPane,
    /// Detach from session
    Detach,
    /// Copy selection to clipboard
    Copy,
    /// Paste from clipboard
    Paste,
    /// Enter scroll/copy mode
    ScrollMode,
    /// Zoom/unzoom current pane
    ZoomPane,
    /// Rename current window
    RenameWindow,
    /// Show window list
    ListWindows,
    /// Send prefix key literally
    SendPrefix,
}

/// Modifier keys
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Modifiers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ctrl() -> Self {
        Self { ctrl: true, ..Default::default() }
    }

    pub fn alt() -> Self {
        Self { alt: true, ..Default::default() }
    }

    pub fn shift() -> Self {
        Self { shift: true, ..Default::default() }
    }

    pub fn none() -> Self {
        Self::default()
    }
}

/// A key with modifiers
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: char,
    pub modifiers: Modifiers,
}

impl KeyCombo {
    pub fn new(key: char, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    pub fn plain(key: char) -> Self {
        Self { key, modifiers: Modifiers::none() }
    }

    pub fn ctrl(key: char) -> Self {
        Self { key, modifiers: Modifiers::ctrl() }
    }

    /// Parse a key combo string like "ctrl+b" or "a"
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('+').collect();
        if parts.is_empty() {
            return None;
        }

        let mut modifiers = Modifiers::default();
        let mut key = None;

        for part in parts {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" | "c" if part.len() > 1 => modifiers.ctrl = true,
                "alt" | "meta" | "option" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                s if s.len() == 1 => key = s.chars().next(),
                _ => {}
            }
        }

        key.map(|k| Self { key: k, modifiers })
    }
}

/// Binding manager state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingState {
    /// Waiting for input (normal mode)
    Normal,
    /// Prefix key was pressed, waiting for command key
    PrefixActive,
}

/// Key bindings configuration and dispatch
pub struct Bindings {
    /// The prefix key combo (default: Ctrl+B)
    prefix: KeyCombo,
    /// Post-prefix bindings (key after prefix -> action)
    bindings: HashMap<char, Action>,
    /// Direct bindings (no prefix needed)
    direct_bindings: HashMap<KeyCombo, Action>,
    /// Current state
    state: BindingState,
}

impl Default for Bindings {
    fn default() -> Self {
        Self::new()
    }
}

impl Bindings {
    /// Create a new binding manager with default bindings
    pub fn new() -> Self {
        let mut bindings = HashMap::new();
        
        // Default tmux-style bindings (after prefix)
        bindings.insert('c', Action::NewWindow);
        bindings.insert('"', Action::SplitHorizontal);
        bindings.insert('%', Action::SplitVertical);
        bindings.insert('n', Action::NextWindow);
        bindings.insert('p', Action::PrevWindow);
        bindings.insert('0', Action::SelectWindow(0));
        bindings.insert('1', Action::SelectWindow(1));
        bindings.insert('2', Action::SelectWindow(2));
        bindings.insert('3', Action::SelectWindow(3));
        bindings.insert('4', Action::SelectWindow(4));
        bindings.insert('5', Action::SelectWindow(5));
        bindings.insert('6', Action::SelectWindow(6));
        bindings.insert('7', Action::SelectWindow(7));
        bindings.insert('8', Action::SelectWindow(8));
        bindings.insert('9', Action::SelectWindow(9));
        bindings.insert('d', Action::Detach);
        bindings.insert('o', Action::NextPane);
        bindings.insert(';', Action::PrevPane);
        bindings.insert('z', Action::ZoomPane);
        bindings.insert(',', Action::RenameWindow);
        bindings.insert('w', Action::ListWindows);
        bindings.insert('[', Action::ScrollMode);
        bindings.insert('b', Action::SendPrefix); // prefix+b sends prefix literally

        let mut direct_bindings = HashMap::new();
        // Shift+Insert for paste (common terminal convention)
        direct_bindings.insert(
            KeyCombo { key: '\x00', modifiers: Modifiers { ctrl: false, alt: false, shift: true } },
            Action::Paste,
        );

        Self {
            prefix: KeyCombo::ctrl('b'),
            bindings,
            direct_bindings,
            state: BindingState::Normal,
        }
    }

    /// Create bindings with a custom prefix
    pub fn with_prefix(prefix: KeyCombo) -> Self {
        let mut b = Self::new();
        b.prefix = prefix;
        b
    }

    /// Set the prefix key from a string like "ctrl+b"
    pub fn set_prefix(&mut self, prefix_str: &str) {
        if let Some(combo) = KeyCombo::parse(prefix_str) {
            self.prefix = combo;
        }
    }

    /// Get the current prefix key
    pub fn prefix(&self) -> KeyCombo {
        self.prefix
    }

    /// Get current binding state
    pub fn state(&self) -> BindingState {
        self.state
    }

    /// Reset state to normal
    pub fn reset_state(&mut self) {
        self.state = BindingState::Normal;
    }

    /// Add or update a post-prefix binding
    pub fn bind(&mut self, key: char, action: Action) {
        self.bindings.insert(key, action);
    }

    /// Add or update a direct binding (no prefix needed)
    pub fn bind_direct(&mut self, combo: KeyCombo, action: Action) {
        self.direct_bindings.insert(combo, action);
    }

    /// Remove a post-prefix binding
    pub fn unbind(&mut self, key: char) {
        self.bindings.remove(&key);
    }

    /// Process a key event, returns an action if one should be triggered
    /// 
    /// Returns:
    /// - Some(action) if a binding was triggered
    /// - None if no binding matched (key should be sent to terminal)
    pub fn process_key(&mut self, key: char, modifiers: Modifiers) -> Option<Action> {
        let combo = KeyCombo { key, modifiers };

        match self.state {
            BindingState::Normal => {
                // Check for direct bindings first
                if let Some(action) = self.direct_bindings.get(&combo) {
                    return Some(action.clone());
                }

                // Check if this is the prefix key
                if combo.key == self.prefix.key && combo.modifiers == self.prefix.modifiers {
                    self.state = BindingState::PrefixActive;
                    return None; // Consume the prefix, don't send to terminal
                }

                // No binding matched
                None
            }
            BindingState::PrefixActive => {
                self.state = BindingState::Normal;

                // Look up the key in post-prefix bindings (ignore modifiers for simplicity)
                if let Some(action) = self.bindings.get(&key) {
                    Some(action.clone())
                } else {
                    // Unknown key after prefix - could beep or ignore
                    None
                }
            }
        }
    }

    /// Check if a key combo matches the prefix
    pub fn is_prefix(&self, key: char, modifiers: Modifiers) -> bool {
        key == self.prefix.key && modifiers == self.prefix.modifiers
    }

    /// Get all post-prefix bindings
    pub fn list_bindings(&self) -> Vec<(char, &Action)> {
        self.bindings.iter().map(|(k, v)| (*k, v)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings() {
        let bindings = Bindings::new();
        assert_eq!(bindings.prefix(), KeyCombo::ctrl('b'));
        assert!(bindings.bindings.contains_key(&'c'));
        assert!(bindings.bindings.contains_key(&'"'));
        assert!(bindings.bindings.contains_key(&'%'));
    }

    #[test]
    fn process_prefix_then_command() {
        let mut bindings = Bindings::new();
        
        // Press Ctrl+B (prefix)
        let result = bindings.process_key('b', Modifiers::ctrl());
        assert!(result.is_none());
        assert_eq!(bindings.state(), BindingState::PrefixActive);

        // Press 'c' (new window)
        let result = bindings.process_key('c', Modifiers::none());
        assert_eq!(result, Some(Action::NewWindow));
        assert_eq!(bindings.state(), BindingState::Normal);
    }

    #[test]
    fn process_prefix_then_split() {
        let mut bindings = Bindings::new();
        
        bindings.process_key('b', Modifiers::ctrl());
        let result = bindings.process_key('"', Modifiers::none());
        assert_eq!(result, Some(Action::SplitHorizontal));
    }

    #[test]
    fn process_prefix_then_window_number() {
        let mut bindings = Bindings::new();
        
        bindings.process_key('b', Modifiers::ctrl());
        let result = bindings.process_key('3', Modifiers::none());
        assert_eq!(result, Some(Action::SelectWindow(3)));
    }

    #[test]
    fn process_unknown_after_prefix() {
        let mut bindings = Bindings::new();
        
        bindings.process_key('b', Modifiers::ctrl());
        let result = bindings.process_key('x', Modifiers::none());
        assert!(result.is_none());
        assert_eq!(bindings.state(), BindingState::Normal);
    }

    #[test]
    fn normal_key_passes_through() {
        let mut bindings = Bindings::new();
        
        // Normal 'a' should pass through
        let result = bindings.process_key('a', Modifiers::none());
        assert!(result.is_none());
        assert_eq!(bindings.state(), BindingState::Normal);
    }

    #[test]
    fn custom_prefix() {
        let mut bindings = Bindings::with_prefix(KeyCombo::ctrl('a'));
        
        // Ctrl+B should NOT activate prefix
        let result = bindings.process_key('b', Modifiers::ctrl());
        assert!(result.is_none());
        assert_eq!(bindings.state(), BindingState::Normal);

        // Ctrl+A should activate prefix
        let result = bindings.process_key('a', Modifiers::ctrl());
        assert!(result.is_none());
        assert_eq!(bindings.state(), BindingState::PrefixActive);
    }

    #[test]
    fn parse_key_combo() {
        let combo = KeyCombo::parse("ctrl+b").unwrap();
        assert_eq!(combo.key, 'b');
        assert!(combo.modifiers.ctrl);
        assert!(!combo.modifiers.alt);

        let combo = KeyCombo::parse("alt+x").unwrap();
        assert_eq!(combo.key, 'x');
        assert!(combo.modifiers.alt);

        let combo = KeyCombo::parse("a").unwrap();
        assert_eq!(combo.key, 'a');
        assert!(!combo.modifiers.ctrl);
    }

    #[test]
    fn set_prefix_from_string() {
        let mut bindings = Bindings::new();
        bindings.set_prefix("ctrl+a");
        assert_eq!(bindings.prefix().key, 'a');
        assert!(bindings.prefix().modifiers.ctrl);
    }

    #[test]
    fn detach_binding() {
        let mut bindings = Bindings::new();
        bindings.process_key('b', Modifiers::ctrl());
        let result = bindings.process_key('d', Modifiers::none());
        assert_eq!(result, Some(Action::Detach));
    }

    #[test]
    fn send_prefix_binding() {
        let mut bindings = Bindings::new();
        bindings.process_key('b', Modifiers::ctrl());
        let result = bindings.process_key('b', Modifiers::none());
        assert_eq!(result, Some(Action::SendPrefix));
    }

    #[test]
    fn list_bindings() {
        let bindings = Bindings::new();
        let list = bindings.list_bindings();
        assert!(!list.is_empty());
        assert!(list.iter().any(|(k, _)| *k == 'c'));
    }
}
