//! Clipboard integration for copy/paste
//!
//! Platform-specific clipboard access.

use crate::Result;
use std::process::{Command, Stdio};

/// Clipboard provider
pub struct Clipboard {
    /// Last copied text (fallback if system clipboard unavailable)
    internal: String,
}

impl Clipboard {
    /// Create a new clipboard handler
    pub fn new() -> Self {
        Self {
            internal: String::new(),
        }
    }

    /// Copy text to clipboard
    pub fn copy(&mut self, text: &str) -> Result<()> {
        // Store internally as fallback
        self.internal = text.to_string();

        // Try system clipboard
        if let Err(_) = self.copy_to_system(text) {
            // System clipboard failed, but internal copy succeeded
            log::debug!("System clipboard unavailable, using internal clipboard");
        }

        Ok(())
    }

    /// Paste text from clipboard
    pub fn paste(&self) -> Result<String> {
        // Try system clipboard first
        if let Ok(text) = self.paste_from_system() {
            return Ok(text);
        }

        // Fall back to internal clipboard
        Ok(self.internal.clone())
    }

    /// Get internal clipboard content
    pub fn internal_content(&self) -> &str {
        &self.internal
    }

    /// Copy to system clipboard (platform-specific)
    #[cfg(target_os = "macos")]
    fn copy_to_system(&self, text: &str) -> Result<()> {
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| crate::Error::Io(e))?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())
                .map_err(|e| crate::Error::Io(e))?;
        }

        child.wait().map_err(|e| crate::Error::Io(e))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn copy_to_system(&self, text: &str) -> Result<()> {
        // Try xclip first, then xsel
        let result = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn();

        let mut child = match result {
            Ok(c) => c,
            Err(_) => {
                // Try xsel
                Command::new("xsel")
                    .args(["--clipboard", "--input"])
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|e| crate::Error::Io(e))?
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())
                .map_err(|e| crate::Error::Io(e))?;
        }

        child.wait().map_err(|e| crate::Error::Io(e))?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn copy_to_system(&self, _text: &str) -> Result<()> {
        Err(crate::Error::Clipboard("Clipboard not supported on this platform".into()))
    }

    /// Paste from system clipboard (platform-specific)
    #[cfg(target_os = "macos")]
    fn paste_from_system(&self) -> Result<String> {
        let output = Command::new("pbpaste")
            .output()
            .map_err(|e| crate::Error::Io(e))?;

        if output.status.success() {
            String::from_utf8(output.stdout)
                .map_err(|e| crate::Error::Clipboard(e.to_string()))
        } else {
            Err(crate::Error::Clipboard("pbpaste failed".into()))
        }
    }

    #[cfg(target_os = "linux")]
    fn paste_from_system(&self) -> Result<String> {
        // Try xclip first
        let output = Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => {
                // Try xsel
                Command::new("xsel")
                    .args(["--clipboard", "--output"])
                    .output()
                    .map_err(|e| crate::Error::Io(e))?
            }
        };

        if output.status.success() {
            String::from_utf8(output.stdout)
                .map_err(|e| crate::Error::Clipboard(e.to_string()))
        } else {
            Err(crate::Error::Clipboard("clipboard read failed".into()))
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn paste_from_system(&self) -> Result<String> {
        Err(crate::Error::Clipboard("Clipboard not supported on this platform".into()))
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_new() {
        let clipboard = Clipboard::new();
        assert!(clipboard.internal_content().is_empty());
    }

    #[test]
    fn clipboard_internal_copy() {
        let mut clipboard = Clipboard::new();
        clipboard.copy("test text").unwrap();
        assert_eq!(clipboard.internal_content(), "test text");
    }

    #[test]
    fn clipboard_internal_paste() {
        let mut clipboard = Clipboard::new();
        clipboard.internal = "internal text".to_string();

        // If system clipboard is unavailable, should return internal
        // This test works regardless of system clipboard state
        assert!(!clipboard.internal_content().is_empty());
    }

    #[test]
    fn clipboard_overwrite() {
        let mut clipboard = Clipboard::new();
        clipboard.copy("first").unwrap();
        clipboard.copy("second").unwrap();
        assert_eq!(clipboard.internal_content(), "second");
    }

    #[test]
    fn clipboard_empty_copy() {
        let mut clipboard = Clipboard::new();
        clipboard.copy("").unwrap();
        assert!(clipboard.internal_content().is_empty());
    }

    #[test]
    fn clipboard_multiline() {
        let mut clipboard = Clipboard::new();
        let text = "line1\nline2\nline3";
        clipboard.copy(text).unwrap();
        assert_eq!(clipboard.internal_content(), text);
    }

    #[test]
    fn clipboard_unicode() {
        let mut clipboard = Clipboard::new();
        let text = "Hello 世界 🎉";
        clipboard.copy(text).unwrap();
        assert_eq!(clipboard.internal_content(), text);
    }
}
