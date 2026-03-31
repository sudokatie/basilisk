//! Key bindings (stub)

use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum Action {
    NewWindow,
    ClosePane,
    SplitHorizontal,
    SplitVertical,
    NextWindow,
    PrevWindow,
    Copy,
    Paste,
}

pub struct Bindings {
    bindings: HashMap<String, Action>,
}

impl Bindings {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub fn default_bindings() -> Self {
        Self::new()
    }
}

impl Default for Bindings {
    fn default() -> Self {
        Self::new()
    }
}
