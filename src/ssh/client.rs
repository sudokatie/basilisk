//! SSH client connection management.
//!
//! Handles connecting to SSH servers, authentication, and channel management.

use super::{AuthMethod, HostKeyAction, HostKeyVerification, KnownHosts, SshTarget};
use crate::error::{Error, Result};
use async_trait::async_trait;
use russh::client::{self, Config, Handle, Handler, Msg};
use russh::{Channel, ChannelId, Disconnect};
// PublicKey from ssh_key crate (re-exported by russh_keys)
type SshPublicKey = russh_keys::ssh_key::PublicKey;
use std::sync::Arc;
use tokio::sync::Mutex;

/// SSH client configuration.
#[derive(Debug, Clone)]
pub struct SshConfig {
    /// Connection timeout in seconds.
    pub timeout_secs: u64,
    /// Keepalive interval in seconds (0 = disabled).
    pub keepalive_secs: u64,
    /// Whether to verify host keys.
    pub verify_host_key: bool,
    /// Callback for unknown host keys.
    pub on_unknown_host: fn(&str, &str) -> HostKeyAction,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            keepalive_secs: 60,
            verify_host_key: true,
            on_unknown_host: |_host, _fp| HostKeyAction::Reject,
        }
    }
}

impl SshConfig {
    /// Create config that accepts all host keys (insecure, for testing).
    pub fn insecure() -> Self {
        Self {
            verify_host_key: false,
            on_unknown_host: |_, _| HostKeyAction::Accept,
            ..Default::default()
        }
    }

    /// Create config that auto-accepts and saves unknown hosts.
    pub fn trust_on_first_use() -> Self {
        Self {
            on_unknown_host: |_, _| HostKeyAction::AcceptAndSave,
            ..Default::default()
        }
    }
}

/// SSH session state.
struct SessionState {
    known_hosts: KnownHosts,
    target: SshTarget,
    config: SshConfig,
    host_key_action: Option<HostKeyAction>,
}

/// SSH error wrapper for russh.
#[derive(Debug)]
struct SshError(Error);

impl std::fmt::Display for SshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for SshError {}

impl From<russh::Error> for SshError {
    fn from(e: russh::Error) -> Self {
        SshError(Error::Ssh(e.to_string()))
    }
}

/// SSH client handler for russh callbacks.
struct SshHandler {
    state: Arc<Mutex<SessionState>>,
}

#[async_trait]
impl Handler for SshHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &SshPublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let mut state = self.state.lock().await;

        // Skip verification if disabled
        if !state.config.verify_host_key {
            return Ok(true);
        }

        // Convert to russh_keys::PublicKey for our verification
        let pub_key: russh_keys::PublicKey = server_public_key.clone();
        
        let verification = state.known_hosts.verify(
            &state.target.host,
            state.target.port,
            &pub_key,
        );

        match verification {
            HostKeyVerification::Verified => Ok(true),
            HostKeyVerification::Unknown { fingerprint } => {
                let host = state.target.host.clone();
                let port = state.target.port;
                let action = (state.config.on_unknown_host)(&host, &fingerprint);
                state.host_key_action = Some(action);
                
                match action {
                    HostKeyAction::Accept => Ok(true),
                    HostKeyAction::AcceptAndSave => {
                        state.known_hosts.add(&host, port, &pub_key);
                        Ok(true)
                    }
                    HostKeyAction::Reject => Ok(false),
                }
            }
            HostKeyVerification::Changed { fingerprint, expected_fingerprint } => {
                Err(SshError(Error::Ssh(format!(
                    "HOST KEY CHANGED for {}: got {}, expected {}. \
                     Possible MITM attack!",
                    state.target.host, fingerprint, expected_fingerprint
                ))))
            }
        }
    }
}

/// Active SSH session.
pub struct SshSession {
    handle: Handle<SshHandler>,
    state: Arc<Mutex<SessionState>>,
}

impl SshSession {
    /// Get the connection target.
    pub async fn target(&self) -> SshTarget {
        self.state.lock().await.target.clone()
    }

    /// Open a shell channel.
    pub async fn shell(&mut self) -> Result<SshChannel> {
        let channel = self.handle.channel_open_session().await.map_err(|e| {
            Error::Ssh(format!("failed to open channel: {}", e))
        })?;

        // Request PTY
        channel.request_pty(
            false,      // want_reply
            "xterm-256color",
            80, 24,     // cols, rows
            0, 0,       // pixel width/height
            &[],        // terminal modes
        ).await.map_err(|e| {
            Error::Ssh(format!("failed to request PTY: {}", e))
        })?;

        // Request shell
        channel.request_shell(false).await.map_err(|e| {
            Error::Ssh(format!("failed to request shell: {}", e))
        })?;

        Ok(SshChannel::new(channel))
    }

    /// Execute a command.
    pub async fn exec(&mut self, command: &str) -> Result<SshChannel> {
        let channel = self.handle.channel_open_session().await.map_err(|e| {
            Error::Ssh(format!("failed to open channel: {}", e))
        })?;

        channel.exec(false, command).await.map_err(|e| {
            Error::Ssh(format!("failed to exec command: {}", e))
        })?;

        Ok(SshChannel::new(channel))
    }

    /// Save known_hosts if any were added.
    pub async fn save_known_hosts(&self) -> Result<()> {
        let state = self.state.lock().await;
        if state.host_key_action == Some(HostKeyAction::AcceptAndSave) {
            state.known_hosts.save()?;
        }
        Ok(())
    }

    /// Disconnect the session.
    pub async fn disconnect(self) -> Result<()> {
        self.handle
            .disconnect(Disconnect::ByApplication, "", "en")
            .await
            .map_err(|e| Error::Ssh(format!("disconnect failed: {}", e)))?;
        Ok(())
    }
}

/// SSH channel for I/O.
pub struct SshChannel {
    channel: Channel<Msg>,
}

impl SshChannel {
    fn new(channel: Channel<Msg>) -> Self {
        Self { channel }
    }

    /// Write data to the channel.
    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        self.channel.data(data).await.map_err(|e| {
            Error::Ssh(format!("write failed: {}", e))
        })?;
        Ok(())
    }

    /// Resize the PTY.
    pub async fn resize(&mut self, cols: u32, rows: u32) -> Result<()> {
        self.channel.window_change(cols, rows, 0, 0).await.map_err(|e| {
            Error::Ssh(format!("resize failed: {}", e))
        })?;
        Ok(())
    }

    /// Close the channel.
    pub async fn close(self) -> Result<()> {
        self.channel.close().await.map_err(|e| {
            Error::Ssh(format!("close failed: {}", e))
        })?;
        Ok(())
    }

    /// Get the channel ID.
    pub fn id(&self) -> ChannelId {
        self.channel.id()
    }
}

/// SSH client for establishing connections.
pub struct SshClient {
    config: SshConfig,
    known_hosts: KnownHosts,
}

impl SshClient {
    /// Create a new SSH client with default configuration.
    pub fn new() -> Result<Self> {
        Ok(Self {
            config: SshConfig::default(),
            known_hosts: KnownHosts::load_default()?,
        })
    }

    /// Create with custom configuration.
    pub fn with_config(config: SshConfig) -> Result<Self> {
        Ok(Self {
            config,
            known_hosts: KnownHosts::load_default()?,
        })
    }

    /// Connect to an SSH server.
    pub async fn connect(
        &self,
        target: &SshTarget,
        auth: &AuthMethod,
    ) -> Result<SshSession> {
        let russh_config = Arc::new(Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(
                self.config.keepalive_secs,
            )),
            ..Default::default()
        });

        let state = Arc::new(Mutex::new(SessionState {
            known_hosts: self.known_hosts.clone(),
            target: target.clone(),
            config: self.config.clone(),
            host_key_action: None,
        }));

        let handler = SshHandler {
            state: Arc::clone(&state),
        };

        // Connect
        let addr = format!("{}:{}", target.host, target.port);
        let mut handle = client::connect(russh_config, &addr, handler)
            .await
            .map_err(|e| Error::Ssh(format!("connection failed: {}", e)))?;

        // Authenticate
        let authenticated = self.authenticate(&mut handle, &target.user, auth).await?;
        if !authenticated {
            return Err(Error::Ssh("authentication failed".to_string()));
        }

        Ok(SshSession { handle, state })
    }

    /// Perform authentication.
    async fn authenticate(
        &self,
        handle: &mut Handle<SshHandler>,
        user: &str,
        auth: &AuthMethod,
    ) -> Result<bool> {
        // Try key authentication first
        for key in auth.load_keys() {
            let auth_key = key.to_auth_key()?;
            let result = handle
                .authenticate_publickey(user, auth_key)
                .await
                .map_err(|e| Error::Ssh(format!("key auth failed: {}", e)))?;
            
            if result {
                return Ok(true);
            }
        }

        // Try password if available
        if let Some(password) = auth.password_value() {
            let result = handle
                .authenticate_password(user, password)
                .await
                .map_err(|e| Error::Ssh(format!("password auth failed: {}", e)))?;
            
            if result {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

impl Default for SshClient {
    fn default() -> Self {
        Self::new().expect("failed to create SSH client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_config_default() {
        let config = SshConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert!(config.verify_host_key);
    }

    #[test]
    fn test_ssh_config_insecure() {
        let config = SshConfig::insecure();
        assert!(!config.verify_host_key);
    }

    #[test]
    fn test_ssh_config_tofu() {
        let config = SshConfig::trust_on_first_use();
        assert!(config.verify_host_key);
        let action = (config.on_unknown_host)("host", "fp");
        assert_eq!(action, HostKeyAction::AcceptAndSave);
    }

    #[test]
    fn test_ssh_config_reject_default() {
        let config = SshConfig::default();
        let action = (config.on_unknown_host)("host", "fp");
        assert_eq!(action, HostKeyAction::Reject);
    }

    #[test]
    fn test_ssh_config_insecure_accepts() {
        let config = SshConfig::insecure();
        let action = (config.on_unknown_host)("host", "fp");
        assert_eq!(action, HostKeyAction::Accept);
    }

    #[test]
    fn test_ssh_config_keepalive() {
        let config = SshConfig::default();
        assert_eq!(config.keepalive_secs, 60);
    }

    #[tokio::test]
    async fn test_client_creation() {
        let client = SshClient::with_config(SshConfig::insecure());
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_client_default() {
        let client = SshClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn test_ssh_error_display() {
        let err = SshError(Error::Ssh("test error".to_string()));
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_ssh_error_from_russh() {
        let russh_err = russh::Error::Disconnect;
        let err: SshError = russh_err.into();
        assert!(err.0.to_string().contains("SSH error"));
    }
}
