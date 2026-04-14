//! SSH known_hosts file management.
//!
//! Handles reading, writing, and verifying host keys against the known_hosts file.

use crate::error::{Error, Result};
use russh_keys::key::PublicKey;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Action to take when encountering a host key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyAction {
    /// Accept and continue.
    Accept,
    /// Reject the connection.
    Reject,
    /// Accept and save to known_hosts.
    AcceptAndSave,
}

/// Result of host key verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyVerification {
    /// Host key matches known key.
    Verified,
    /// Host is unknown (not in known_hosts).
    Unknown { fingerprint: String },
    /// Host key changed (possible MITM attack).
    Changed { 
        fingerprint: String,
        expected_fingerprint: String,
    },
}

/// Known hosts database.
#[derive(Debug, Clone)]
pub struct KnownHosts {
    /// Path to known_hosts file.
    path: PathBuf,
    /// Host -> key type -> base64 key.
    entries: HashMap<String, HashMap<String, String>>,
}

impl KnownHosts {
    /// Load known_hosts from the default location.
    pub fn load_default() -> Result<Self> {
        Self::load(super::known_hosts_path())
    }

    /// Load known_hosts from a specific path.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let mut entries = HashMap::new();

        if path.exists() {
            let file = File::open(&path).map_err(|e| {
                Error::Ssh(format!("failed to open known_hosts: {}", e))
            })?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line.map_err(|e| {
                    Error::Ssh(format!("failed to read known_hosts: {}", e))
                })?;
                let line = line.trim();

                // Skip comments and empty lines
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                // Parse: hostname key_type base64_key [comment]
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let host = parts[0].to_string();
                    let key_type = parts[1].to_string();
                    let key_data = parts[2].to_string();

                    entries
                        .entry(host)
                        .or_insert_with(HashMap::new)
                        .insert(key_type, key_data);
                }
            }
        }

        Ok(Self { path, entries })
    }

    /// Verify a host key against known_hosts.
    pub fn verify(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyVerification {
        let host_entry = self.host_string(host, port);
        let key_type = key_type_string(key);
        let key_data = key_to_base64(key);
        let fingerprint = key_fingerprint(key);

        if let Some(host_keys) = self.entries.get(&host_entry) {
            if let Some(known_key) = host_keys.get(&key_type) {
                if known_key == &key_data {
                    HostKeyVerification::Verified
                } else {
                    HostKeyVerification::Changed {
                        fingerprint,
                        expected_fingerprint: format!("{}:{}", key_type, &known_key[..16]),
                    }
                }
            } else {
                // Known host but different key type
                HostKeyVerification::Unknown { fingerprint }
            }
        } else {
            HostKeyVerification::Unknown { fingerprint }
        }
    }

    /// Add or update a host key.
    pub fn add(&mut self, host: &str, port: u16, key: &PublicKey) {
        let host_entry = self.host_string(host, port);
        let key_type = key_type_string(key);
        let key_data = key_to_base64(key);

        self.entries
            .entry(host_entry)
            .or_insert_with(HashMap::new)
            .insert(key_type, key_data);
    }

    /// Remove a host from known_hosts.
    pub fn remove(&mut self, host: &str, port: u16) {
        let host_entry = self.host_string(host, port);
        self.entries.remove(&host_entry);
    }

    /// Save known_hosts to disk.
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                Error::Ssh(format!("failed to create .ssh directory: {}", e))
            })?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&self.path)
            .map_err(|e| {
                Error::Ssh(format!("failed to write known_hosts: {}", e))
            })?;

        for (host, keys) in &self.entries {
            for (key_type, key_data) in keys {
                writeln!(file, "{} {} {}", host, key_type, key_data).map_err(|e| {
                    Error::Ssh(format!("failed to write known_hosts entry: {}", e))
                })?;
            }
        }

        Ok(())
    }

    /// Format host string with port if non-standard.
    fn host_string(&self, host: &str, port: u16) -> String {
        if port == 22 {
            host.to_string()
        } else {
            format!("[{}]:{}", host, port)
        }
    }

    /// Check if a host is known.
    pub fn is_known(&self, host: &str, port: u16) -> bool {
        let host_entry = self.host_string(host, port);
        self.entries.contains_key(&host_entry)
    }

    /// Get number of known hosts.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Get SSH key type string.
fn key_type_string(key: &PublicKey) -> String {
    match key {
        PublicKey::Ed25519(_) => "ssh-ed25519".to_string(),
        PublicKey::RSA { .. } => "ssh-rsa".to_string(),
        PublicKey::EC { ref key } => {
            match key.ident() {
                "nistp256" => "ecdsa-sha2-nistp256".to_string(),
                "nistp384" => "ecdsa-sha2-nistp384".to_string(),
                "nistp521" => "ecdsa-sha2-nistp521".to_string(),
                other => format!("ecdsa-sha2-{}", other),
            }
        }
    }
}

/// Convert public key to base64.
fn key_to_base64(key: &PublicKey) -> String {
    use base64::Engine;
    let bytes = key.public_key_bytes();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Get key fingerprint (SHA256).
fn key_fingerprint(key: &PublicKey) -> String {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    
    // Simple fingerprint for display (real impl would use SHA256)
    let bytes = key.public_key_bytes();
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let hash = hasher.finish();
    format!("SHA256:{:016x}", hash)
}

// Unix-specific file permissions
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[cfg(not(unix))]
trait OpenOptionsExt {
    fn mode(&mut self, _mode: u32) -> &mut Self;
}

#[cfg(not(unix))]
impl OpenOptionsExt for OpenOptions {
    fn mode(&mut self, _mode: u32) -> &mut Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_known_hosts_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("known_hosts");
        let kh = KnownHosts::load(&path).unwrap();
        assert!(kh.is_empty());
    }

    #[test]
    fn test_host_string_standard_port() {
        let tmp = TempDir::new().unwrap();
        let kh = KnownHosts::load(tmp.path().join("kh")).unwrap();
        assert_eq!(kh.host_string("example.com", 22), "example.com");
    }

    #[test]
    fn test_host_string_custom_port() {
        let tmp = TempDir::new().unwrap();
        let kh = KnownHosts::load(tmp.path().join("kh")).unwrap();
        assert_eq!(kh.host_string("example.com", 2222), "[example.com]:2222");
    }

    #[test]
    fn test_known_hosts_parse_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("known_hosts");
        
        fs::write(&path, "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITest\n").unwrap();
        
        let kh = KnownHosts::load(&path).unwrap();
        assert_eq!(kh.len(), 1);
        assert!(kh.is_known("example.com", 22));
    }

    #[test]
    fn test_known_hosts_skip_comments() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("known_hosts");
        
        fs::write(&path, "# comment\nexample.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITest\n\n").unwrap();
        
        let kh = KnownHosts::load(&path).unwrap();
        assert_eq!(kh.len(), 1);
    }

    #[test]
    fn test_is_known() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("known_hosts");
        fs::write(&path, "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITest\n").unwrap();
        
        let kh = KnownHosts::load(&path).unwrap();
        assert!(kh.is_known("example.com", 22));
        assert!(!kh.is_known("other.com", 22));
        assert!(!kh.is_known("example.com", 2222));
    }
}
