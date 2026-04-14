//! SSH authentication methods.
//!
//! Supports key-based and password authentication.

use crate::error::{Error, Result};
use russh_keys::key::PrivateKeyWithHashAlg;
use russh_keys::PrivateKey;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// SSH key pair wrapper.
#[derive(Clone)]
pub struct SshKeyPair {
    inner: Arc<PrivateKey>,
    path: Option<PathBuf>,
}

impl SshKeyPair {
    /// Load a key pair from file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        Self::load_with_passphrase(path, None)
    }

    /// Load a key pair with optional passphrase.
    pub fn load_with_passphrase(
        path: impl AsRef<Path>,
        passphrase: Option<&str>,
    ) -> Result<Self> {
        let path = path.as_ref();
        let key_data = std::fs::read_to_string(path).map_err(|e| {
            Error::Ssh(format!("failed to read key {}: {}", path.display(), e))
        })?;

        let key = if let Some(pass) = passphrase {
            PrivateKey::from_openssh(&key_data)
                .and_then(|k| k.decrypt(pass.as_bytes()))
                .map_err(|e| Error::Ssh(format!("failed to decrypt key: {}", e)))?
        } else {
            PrivateKey::from_openssh(&key_data)
                .map_err(|e| Error::Ssh(format!("failed to parse key {}: {}", path.display(), e)))?
        };

        Ok(Self {
            inner: Arc::new(key),
            path: Some(path.to_path_buf()),
        })
    }

    /// Get the key algorithm name.
    pub fn algorithm(&self) -> &'static str {
        match self.inner.algorithm() {
            russh_keys::Algorithm::Ed25519 => "ed25519",
            russh_keys::Algorithm::Rsa { .. } => "rsa",
            russh_keys::Algorithm::Ecdsa { .. } => "ecdsa",
            _ => "unknown",
        }
    }

    /// Get the path this key was loaded from.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Get the inner key for russh authentication.
    pub(crate) fn to_auth_key(&self) -> Result<PrivateKeyWithHashAlg> {
        PrivateKeyWithHashAlg::new(Arc::clone(&self.inner), None)
            .map_err(|e| Error::Ssh(format!("failed to create auth key: {}", e)))
    }
}

impl std::fmt::Debug for SshKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshKeyPair")
            .field("algorithm", &self.algorithm())
            .field("path", &self.path)
            .finish()
    }
}

/// Authentication method for SSH connection.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// Key-based authentication.
    PublicKey {
        /// Key pairs to try.
        keys: Vec<SshKeyPair>,
    },
    /// Password authentication.
    Password {
        /// The password.
        password: String,
    },
    /// Try keys, then password.
    Auto {
        /// Key files to try.
        key_files: Vec<PathBuf>,
        /// Optional password fallback.
        password: Option<String>,
    },
    /// No authentication (for testing).
    None,
}

impl AuthMethod {
    /// Create key-based auth from a single key file.
    pub fn key(path: impl AsRef<Path>) -> Result<Self> {
        let key = SshKeyPair::load(path)?;
        Ok(Self::PublicKey { keys: vec![key] })
    }

    /// Create key-based auth from multiple key files.
    pub fn keys(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Result<Self> {
        let keys: Result<Vec<_>> = paths
            .into_iter()
            .map(|p| SshKeyPair::load(p))
            .collect();
        Ok(Self::PublicKey { keys: keys? })
    }

    /// Create password authentication.
    pub fn password(password: impl Into<String>) -> Self {
        Self::Password {
            password: password.into(),
        }
    }

    /// Create auto authentication (tries default keys).
    pub fn auto() -> Self {
        Self::Auto {
            key_files: super::default_identity_files(),
            password: None,
        }
    }

    /// Create auto authentication with password fallback.
    pub fn auto_with_password(password: impl Into<String>) -> Self {
        Self::Auto {
            key_files: super::default_identity_files(),
            password: Some(password.into()),
        }
    }

    /// Load keys for this auth method.
    pub fn load_keys(&self) -> Vec<SshKeyPair> {
        match self {
            Self::PublicKey { keys } => keys.clone(),
            Self::Auto { key_files, .. } => {
                key_files
                    .iter()
                    .filter_map(|p| {
                        if p.exists() {
                            SshKeyPair::load(p).ok()
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    /// Get password if available.
    pub fn password_value(&self) -> Option<&str> {
        match self {
            Self::Password { password } => Some(password),
            Self::Auto { password, .. } => password.as_deref(),
            _ => None,
        }
    }
}

impl Default for AuthMethod {
    fn default() -> Self {
        Self::auto()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_method_password() {
        let auth = AuthMethod::password("secret");
        assert_eq!(auth.password_value(), Some("secret"));
        assert!(auth.load_keys().is_empty());
    }

    #[test]
    fn test_auth_method_auto() {
        let auth = AuthMethod::auto();
        match auth {
            AuthMethod::Auto { key_files, password } => {
                assert!(!key_files.is_empty());
                assert!(password.is_none());
            }
            _ => panic!("expected Auto"),
        }
    }

    #[test]
    fn test_auth_method_auto_with_password() {
        let auth = AuthMethod::auto_with_password("fallback");
        assert_eq!(auth.password_value(), Some("fallback"));
    }

    #[test]
    fn test_auth_method_none() {
        let auth = AuthMethod::None;
        assert!(auth.password_value().is_none());
        assert!(auth.load_keys().is_empty());
    }
}
