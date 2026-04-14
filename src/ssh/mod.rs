//! SSH client integration for Basilisk terminal.
//!
//! Provides built-in SSH connectivity without spawning external ssh processes.
//! Supports key-based authentication, connection multiplexing, and known_hosts
//! verification.

mod auth;
mod client;
mod known_hosts;

pub use auth::{AuthMethod, KeyPair};
pub use client::{SshClient, SshConfig, SshSession, SshChannel};
pub use known_hosts::{KnownHosts, HostKeyVerification, HostKeyAction};

use crate::error::{Error, Result};
use std::path::PathBuf;

/// Default SSH port.
pub const DEFAULT_PORT: u16 = 22;

/// SSH connection target.
#[derive(Debug, Clone)]
pub struct SshTarget {
    /// Hostname or IP address.
    pub host: String,
    /// Port number (default: 22).
    pub port: u16,
    /// Username for authentication.
    pub user: String,
}

impl SshTarget {
    /// Create a new SSH target.
    pub fn new(host: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: DEFAULT_PORT,
            user: user.into(),
        }
    }

    /// Set the port number.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Parse from user@host:port format.
    pub fn parse(s: &str) -> Result<Self> {
        // Format: [user@]host[:port]
        let (user_part, rest) = if let Some(idx) = s.find('@') {
            (Some(&s[..idx]), &s[idx + 1..])
        } else {
            (None, s)
        };

        let (host, port) = if let Some(idx) = rest.rfind(':') {
            let port_str = &rest[idx + 1..];
            let port = port_str.parse::<u16>().map_err(|_| {
                Error::Ssh(format!("invalid port number: {}", port_str))
            })?;
            (&rest[..idx], port)
        } else {
            (rest, DEFAULT_PORT)
        };

        let user = user_part
            .map(String::from)
            .or_else(|| std::env::var("USER").ok())
            .unwrap_or_else(|| "root".to_string());

        Ok(Self {
            host: host.to_string(),
            port,
            user,
        })
    }

    /// Format as user@host:port string.
    pub fn to_string_full(&self) -> String {
        format!("{}@{}:{}", self.user, self.host, self.port)
    }
}

impl std::fmt::Display for SshTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.port == DEFAULT_PORT {
            write!(f, "{}@{}", self.user, self.host)
        } else {
            write!(f, "{}@{}:{}", self.user, self.host, self.port)
        }
    }
}

/// Get the default SSH directory (~/.ssh).
pub fn ssh_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ssh"))
        .unwrap_or_else(|| PathBuf::from(".ssh"))
}

/// Get the default known_hosts file path.
pub fn known_hosts_path() -> PathBuf {
    ssh_dir().join("known_hosts")
}

/// Get the default identity files to try.
pub fn default_identity_files() -> Vec<PathBuf> {
    let dir = ssh_dir();
    vec![
        dir.join("id_ed25519"),
        dir.join("id_rsa"),
        dir.join("id_ecdsa"),
        dir.join("id_dsa"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_new() {
        let target = SshTarget::new("example.com", "admin");
        assert_eq!(target.host, "example.com");
        assert_eq!(target.user, "admin");
        assert_eq!(target.port, 22);
    }

    #[test]
    fn test_target_with_port() {
        let target = SshTarget::new("example.com", "admin").with_port(2222);
        assert_eq!(target.port, 2222);
    }

    #[test]
    fn test_target_parse_full() {
        let target = SshTarget::parse("user@host.com:2222").unwrap();
        assert_eq!(target.user, "user");
        assert_eq!(target.host, "host.com");
        assert_eq!(target.port, 2222);
    }

    #[test]
    fn test_target_parse_no_port() {
        let target = SshTarget::parse("user@host.com").unwrap();
        assert_eq!(target.user, "user");
        assert_eq!(target.host, "host.com");
        assert_eq!(target.port, 22);
    }

    #[test]
    fn test_target_parse_host_only() {
        let target = SshTarget::parse("host.com").unwrap();
        assert_eq!(target.host, "host.com");
        assert_eq!(target.port, 22);
    }

    #[test]
    fn test_target_display() {
        let target = SshTarget::new("example.com", "admin");
        assert_eq!(format!("{}", target), "admin@example.com");

        let target_port = target.with_port(2222);
        assert_eq!(format!("{}", target_port), "admin@example.com:2222");
    }

    #[test]
    fn test_default_paths() {
        let dir = ssh_dir();
        assert!(dir.to_string_lossy().contains(".ssh"));

        let known = known_hosts_path();
        assert!(known.to_string_lossy().contains("known_hosts"));
    }
}
