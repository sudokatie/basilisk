//! Terminal bell implementation
//!
//! Provides audible and visual bell functionality.

use std::time::{Duration, Instant};

/// Bell configuration
#[derive(Debug, Clone)]
pub struct BellConfig {
    /// Enable audible bell
    pub audible: bool,
    /// Enable visual bell (flash)
    pub visual: bool,
    /// Duration of visual bell flash in milliseconds
    pub visual_duration_ms: u64,
    /// Minimum interval between bells (rate limiting)
    pub min_interval_ms: u64,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self {
            audible: true,
            visual: true,
            visual_duration_ms: 100,
            min_interval_ms: 100, // Max 10 bells per second
        }
    }
}

/// Bell state and handler
pub struct Bell {
    config: BellConfig,
    /// Last bell time for rate limiting
    last_bell: Option<Instant>,
    /// Visual bell active until this time
    visual_active_until: Option<Instant>,
}

impl Default for Bell {
    fn default() -> Self {
        Self::new(BellConfig::default())
    }
}

impl Bell {
    /// Create a new bell handler with configuration
    pub fn new(config: BellConfig) -> Self {
        Self {
            config,
            last_bell: None,
            visual_active_until: None,
        }
    }

    /// Trigger the bell
    pub fn ring(&mut self) {
        let now = Instant::now();

        // Rate limiting
        if let Some(last) = self.last_bell {
            if now.duration_since(last) < Duration::from_millis(self.config.min_interval_ms) {
                return;
            }
        }

        self.last_bell = Some(now);

        // Audible bell
        if self.config.audible {
            self.audible_bell();
        }

        // Visual bell
        if self.config.visual {
            self.visual_active_until = Some(
                now + Duration::from_millis(self.config.visual_duration_ms)
            );
        }
    }

    /// Check if visual bell is currently active
    pub fn is_visual_active(&self) -> bool {
        match self.visual_active_until {
            Some(until) => Instant::now() < until,
            None => false,
        }
    }

    /// Clear visual bell state (call when flash duration expires)
    pub fn update(&mut self) {
        if let Some(until) = self.visual_active_until {
            if Instant::now() >= until {
                self.visual_active_until = None;
            }
        }
    }

    /// Trigger audible bell (platform-specific)
    #[cfg(target_os = "macos")]
    fn audible_bell(&self) {
        // Use NSBeep via command line (safest cross-version approach)
        let _ = std::process::Command::new("osascript")
            .args(["-e", "beep"])
            .spawn();
    }

    #[cfg(target_os = "linux")]
    fn audible_bell(&self) {
        // Try paplay first (PulseAudio), then aplay (ALSA), then fallback to print \a
        let result = std::process::Command::new("paplay")
            .arg("/usr/share/sounds/freedesktop/stereo/bell.oga")
            .spawn();

        if result.is_err() {
            // Fallback: try writing BEL to /dev/console or tty
            let _ = std::process::Command::new("sh")
                .args(["-c", "echo -ne '\\a'"])
                .spawn();
        }
    }

    #[cfg(target_os = "windows")]
    fn audible_bell(&self) {
        // Windows MessageBeep
        unsafe {
            // winapi::um::winuser::MessageBeep(0xFFFFFFFF); // MB_OK
            // For now, use a simple command
            let _ = std::process::Command::new("cmd")
                .args(["/c", "echo \x07"])
                .spawn();
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    fn audible_bell(&self) {
        // Generic: print BEL character to stderr
        eprint!("\x07");
    }

    /// Get the visual bell flash color multiplier (for rendering)
    /// Returns 1.0 normally, higher values during flash
    pub fn visual_intensity(&self) -> f32 {
        if !self.is_visual_active() {
            return 1.0;
        }

        // Calculate intensity based on time remaining
        if let Some(until) = self.visual_active_until {
            let now = Instant::now();
            if now < until {
                let remaining = until.duration_since(now);
                let total = Duration::from_millis(self.config.visual_duration_ms);
                let progress = 1.0 - (remaining.as_secs_f32() / total.as_secs_f32());
                // Flash intensity: starts high, fades out
                return 1.0 + (1.0 - progress) * 0.3;
            }
        }

        1.0
    }

    /// Set configuration
    pub fn set_config(&mut self, config: BellConfig) {
        self.config = config;
    }

    /// Get configuration
    pub fn config(&self) -> &BellConfig {
        &self.config
    }

    /// Enable/disable audible bell
    pub fn set_audible(&mut self, enabled: bool) {
        self.config.audible = enabled;
    }

    /// Enable/disable visual bell
    pub fn set_visual(&mut self, enabled: bool) {
        self.config.visual = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn bell_config_default() {
        let config = BellConfig::default();
        assert!(config.audible);
        assert!(config.visual);
    }

    #[test]
    fn bell_new() {
        let bell = Bell::default();
        assert!(!bell.is_visual_active());
    }

    #[test]
    fn bell_visual_active() {
        let mut bell = Bell::default();
        bell.ring();
        
        // Visual bell should be active immediately after ring
        assert!(bell.is_visual_active());
    }

    #[test]
    fn bell_visual_expires() {
        let config = BellConfig {
            audible: false, // Don't make noise in tests
            visual: true,
            visual_duration_ms: 10, // Short duration for testing
            min_interval_ms: 0,
        };
        let mut bell = Bell::new(config);
        bell.ring();
        
        assert!(bell.is_visual_active());
        
        // Wait for expiry
        sleep(Duration::from_millis(20));
        bell.update();
        
        assert!(!bell.is_visual_active());
    }

    #[test]
    fn bell_rate_limiting() {
        let config = BellConfig {
            audible: false,
            visual: true,
            visual_duration_ms: 100,
            min_interval_ms: 100,
        };
        let mut bell = Bell::new(config);
        
        bell.ring();
        let first_until = bell.visual_active_until;
        
        // Ring again immediately - should be rate limited
        bell.ring();
        
        // visual_active_until should not change (bell was rate limited)
        assert_eq!(bell.visual_active_until, first_until);
    }

    #[test]
    fn bell_intensity() {
        let config = BellConfig {
            audible: false,
            visual: true,
            visual_duration_ms: 100,
            min_interval_ms: 0,
        };
        let mut bell = Bell::new(config);
        
        // Before ring: intensity is 1.0
        assert!((bell.visual_intensity() - 1.0).abs() < 0.01);
        
        // After ring: intensity should be > 1.0
        bell.ring();
        assert!(bell.visual_intensity() > 1.0);
    }

    #[test]
    fn bell_set_config() {
        let mut bell = Bell::default();
        
        bell.set_audible(false);
        assert!(!bell.config().audible);
        
        bell.set_visual(false);
        assert!(!bell.config().visual);
    }

    #[test]
    fn bell_disabled_visual() {
        let config = BellConfig {
            audible: false,
            visual: false,
            visual_duration_ms: 100,
            min_interval_ms: 0,
        };
        let mut bell = Bell::new(config);
        
        bell.ring();
        
        // Visual should not be active when disabled
        assert!(!bell.is_visual_active());
    }
}
