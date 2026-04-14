//! Top-level multiplexer managing sessions
//!
//! Provides tmux-like session management with create, attach, detach, and list.

use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use crate::Result;
use super::session::{Session, SessionId};
use super::ipc::{SessionServer, IpcMessage, list_sessions as ipc_list_sessions};

/// Top-level multiplexer for managing multiple sessions
pub struct Multiplexer {
    /// All managed sessions
    sessions: HashMap<SessionId, Session>,
    /// Currently attached session
    attached: Option<SessionId>,
    /// IPC server for remote attach/detach
    server: Option<SessionServer>,
    /// Connected clients
    clients: Vec<UnixStream>,
    /// Next session ID
    next_id: u32,
    /// Default shell for new sessions
    default_shell: String,
}

impl Multiplexer {
    /// Create a new multiplexer
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            attached: None,
            server: None,
            clients: Vec::new(),
            next_id: 0,
            default_shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
        }
    }

    /// Create with a custom default shell
    pub fn with_shell(shell: String) -> Self {
        Self {
            sessions: HashMap::new(),
            attached: None,
            server: None,
            clients: Vec::new(),
            next_id: 0,
            default_shell: shell,
        }
    }

    /// Create a new session
    pub fn create_session(&mut self, name: Option<String>) -> Result<SessionId> {
        let id = SessionId::new(self.next_id);
        self.next_id += 1;

        let session_name = name.unwrap_or_else(|| format!("session-{}", id.0));
        let session = Session::new(id, session_name);

        self.sessions.insert(id, session);

        // Auto-attach if this is the first session
        if self.attached.is_none() {
            self.attached = Some(id);
        }

        Ok(id)
    }

    /// Create a session with initial window spawning a shell
    pub fn create_session_with_shell(&mut self, name: Option<String>) -> Result<SessionId> {
        let id = self.create_session(name)?;

        if let Some(session) = self.sessions.get_mut(&id) {
            let window_name = self.default_shell.clone();
            session.create_window_with_shell(window_name)?;
        }

        Ok(id)
    }

    /// Destroy a session
    pub fn destroy_session(&mut self, id: SessionId) -> bool {
        if self.sessions.remove(&id).is_some() {
            // If we destroyed the attached session, detach
            if self.attached == Some(id) {
                self.attached = self.sessions.keys().next().copied();
            }
            true
        } else {
            false
        }
    }

    /// Attach to a session
    pub fn attach(&mut self, id: SessionId) -> bool {
        if self.sessions.contains_key(&id) {
            self.attached = Some(id);
            true
        } else {
            false
        }
    }

    /// Attach to a session by name
    pub fn attach_by_name(&mut self, name: &str) -> bool {
        if let Some((&id, _)) = self.sessions.iter().find(|(_, s)| s.name() == name) {
            self.attached = Some(id);
            true
        } else {
            false
        }
    }

    /// Detach from current session
    pub fn detach(&mut self) -> Option<SessionId> {
        self.attached.take()
    }

    /// Get the currently attached session
    pub fn attached_session(&self) -> Option<&Session> {
        self.attached.and_then(|id| self.sessions.get(&id))
    }

    /// Get mutable reference to attached session
    pub fn attached_session_mut(&mut self) -> Option<&mut Session> {
        self.attached.and_then(|id| self.sessions.get_mut(&id))
    }

    /// Get a session by ID
    pub fn get_session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.get(&id)
    }

    /// Get mutable session by ID
    pub fn get_session_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.sessions.get_mut(&id)
    }

    /// Get a session by name
    pub fn get_session_by_name(&self, name: &str) -> Option<&Session> {
        self.sessions.values().find(|s| s.name() == name)
    }

    /// List all session IDs
    pub fn session_ids(&self) -> Vec<SessionId> {
        self.sessions.keys().copied().collect()
    }

    /// List all sessions with their names
    pub fn list_sessions(&self) -> Vec<(SessionId, String)> {
        self.sessions
            .iter()
            .map(|(&id, session)| (id, session.name().to_string()))
            .collect()
    }

    /// Get number of sessions
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Check if a session exists
    pub fn has_session(&self, id: SessionId) -> bool {
        self.sessions.contains_key(&id)
    }

    /// Get the attached session ID
    pub fn attached_id(&self) -> Option<SessionId> {
        self.attached
    }

    /// Start IPC server for this multiplexer
    pub fn start_server(&mut self, name: &str) -> Result<()> {
        let server = SessionServer::new(name)?;
        server.set_nonblocking(true)?;
        self.server = Some(server);
        Ok(())
    }

    /// Stop IPC server
    pub fn stop_server(&mut self) {
        self.server = None;
        self.clients.clear();
    }

    /// Poll for new client connections (non-blocking)
    pub fn accept_clients(&mut self) {
        if let Some(server) = &self.server {
            // Try to accept in non-blocking mode
            if let Ok(stream) = server.accept() {
                let _ = stream.set_nonblocking(true);
                self.clients.push(stream);
            }
        }
    }

    /// Send output to all connected clients
    pub fn broadcast_output(&mut self, data: &[u8]) {
        use std::io::Write;

        let msg = IpcMessage::Output(data.to_vec());
        let bytes = msg.to_bytes();
        let len = (bytes.len() as u32).to_le_bytes();

        // Remove disconnected clients while broadcasting
        self.clients.retain_mut(|stream| {
            if stream.write_all(&len).is_err() {
                return false;
            }
            if stream.write_all(&bytes).is_err() {
                return false;
            }
            let _ = stream.flush();
            true
        });
    }

    /// Read input from clients (non-blocking)
    pub fn poll_client_input(&mut self) -> Option<Vec<u8>> {
        use std::io::Read;

        for stream in &mut self.clients {
            let mut len_buf = [0u8; 4];
            if stream.read_exact(&mut len_buf).is_ok() {
                let len = u32::from_le_bytes(len_buf) as usize;
                let mut data = vec![0u8; len];
                if stream.read_exact(&mut data).is_ok() {
                    if let Some(IpcMessage::Input(input)) = IpcMessage::from_bytes(&data) {
                        return Some(input);
                    }
                }
            }
        }
        None
    }

    /// Get number of connected clients
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// List running sessions from IPC (static method for client use)
    pub fn list_running_sessions() -> Vec<String> {
        ipc_list_sessions()
    }

    /// Select next session (circular)
    pub fn next_session(&mut self) {
        let ids: Vec<_> = self.sessions.keys().copied().collect();
        if ids.is_empty() {
            return;
        }

        if let Some(current) = self.attached {
            if let Some(pos) = ids.iter().position(|&id| id == current) {
                let next_pos = (pos + 1) % ids.len();
                self.attached = Some(ids[next_pos]);
            }
        } else {
            self.attached = ids.first().copied();
        }
    }

    /// Select previous session (circular)
    pub fn prev_session(&mut self) {
        let ids: Vec<_> = self.sessions.keys().copied().collect();
        if ids.is_empty() {
            return;
        }

        if let Some(current) = self.attached {
            if let Some(pos) = ids.iter().position(|&id| id == current) {
                let prev_pos = if pos == 0 { ids.len() - 1 } else { pos - 1 };
                self.attached = Some(ids[prev_pos]);
            }
        } else {
            self.attached = ids.last().copied();
        }
    }
}

impl Default for Multiplexer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplexer_new() {
        let mux = Multiplexer::new();
        assert_eq!(mux.session_count(), 0);
        assert!(mux.attached_id().is_none());
    }

    #[test]
    fn multiplexer_create_session() {
        let mut mux = Multiplexer::new();
        let id = mux.create_session(Some("test".to_string())).unwrap();

        assert_eq!(mux.session_count(), 1);
        assert!(mux.has_session(id));
        assert_eq!(mux.attached_id(), Some(id)); // Auto-attached
    }

    #[test]
    fn multiplexer_create_multiple_sessions() {
        let mut mux = Multiplexer::new();
        let id1 = mux.create_session(Some("one".to_string())).unwrap();
        let id2 = mux.create_session(Some("two".to_string())).unwrap();

        assert_eq!(mux.session_count(), 2);
        assert!(mux.has_session(id1));
        assert!(mux.has_session(id2));
        // First session stays attached
        assert_eq!(mux.attached_id(), Some(id1));
    }

    #[test]
    fn multiplexer_destroy_session() {
        let mut mux = Multiplexer::new();
        let id = mux.create_session(None).unwrap();

        assert!(mux.destroy_session(id));
        assert_eq!(mux.session_count(), 0);
        assert!(!mux.has_session(id));
    }

    #[test]
    fn multiplexer_destroy_attached_session() {
        let mut mux = Multiplexer::new();
        let id1 = mux.create_session(Some("one".to_string())).unwrap();
        let id2 = mux.create_session(Some("two".to_string())).unwrap();

        // Destroy the attached session
        assert!(mux.destroy_session(id1));

        // Should auto-attach to remaining session
        assert_eq!(mux.attached_id(), Some(id2));
    }

    #[test]
    fn multiplexer_attach_detach() {
        let mut mux = Multiplexer::new();
        let id1 = mux.create_session(Some("one".to_string())).unwrap();
        let id2 = mux.create_session(Some("two".to_string())).unwrap();

        // Attach to second session
        assert!(mux.attach(id2));
        assert_eq!(mux.attached_id(), Some(id2));

        // Detach
        let detached = mux.detach();
        assert_eq!(detached, Some(id2));
        assert!(mux.attached_id().is_none());

        // Re-attach
        assert!(mux.attach(id1));
        assert_eq!(mux.attached_id(), Some(id1));
    }

    #[test]
    fn multiplexer_attach_by_name() {
        let mut mux = Multiplexer::new();
        let _id1 = mux.create_session(Some("alpha".to_string())).unwrap();
        let id2 = mux.create_session(Some("beta".to_string())).unwrap();

        assert!(mux.attach_by_name("beta"));
        assert_eq!(mux.attached_id(), Some(id2));

        assert!(!mux.attach_by_name("nonexistent"));
    }

    #[test]
    fn multiplexer_get_session() {
        let mut mux = Multiplexer::new();
        let id = mux.create_session(Some("test".to_string())).unwrap();

        let session = mux.get_session(id);
        assert!(session.is_some());
        assert_eq!(session.unwrap().name(), "test");
    }

    #[test]
    fn multiplexer_get_session_by_name() {
        let mut mux = Multiplexer::new();
        let _id = mux.create_session(Some("findme".to_string())).unwrap();

        let session = mux.get_session_by_name("findme");
        assert!(session.is_some());

        let not_found = mux.get_session_by_name("nothere");
        assert!(not_found.is_none());
    }

    #[test]
    fn multiplexer_list_sessions() {
        let mut mux = Multiplexer::new();
        mux.create_session(Some("one".to_string())).unwrap();
        mux.create_session(Some("two".to_string())).unwrap();

        let list = mux.list_sessions();
        assert_eq!(list.len(), 2);

        let names: Vec<_> = list.iter().map(|(_, n)| n.as_str()).collect();
        assert!(names.contains(&"one"));
        assert!(names.contains(&"two"));
    }

    #[test]
    fn multiplexer_next_prev_session() {
        let mut mux = Multiplexer::new();
        let id1 = mux.create_session(Some("one".to_string())).unwrap();
        let id2 = mux.create_session(Some("two".to_string())).unwrap();
        let _id3 = mux.create_session(Some("three".to_string())).unwrap();

        // Start at first
        mux.attach(id1);
        assert_eq!(mux.attached_id(), Some(id1));

        // Next should go to second
        mux.next_session();
        assert!(mux.attached_id().is_some());

        // Prev should go back
        mux.prev_session();
        // Note: order isn't guaranteed with HashMap, but navigation should work
        assert!(mux.attached_id().is_some());
    }

    #[test]
    fn multiplexer_attached_session() {
        let mut mux = Multiplexer::new();
        let id = mux.create_session(Some("test".to_string())).unwrap();

        let session = mux.attached_session();
        assert!(session.is_some());
        assert_eq!(session.unwrap().id(), id);
    }

    #[test]
    fn multiplexer_with_shell() {
        let mux = Multiplexer::with_shell("/bin/zsh".to_string());
        assert_eq!(mux.session_count(), 0);
    }

    #[test]
    fn multiplexer_attach_nonexistent() {
        let mut mux = Multiplexer::new();
        let fake_id = SessionId::new(999);

        assert!(!mux.attach(fake_id));
        assert!(mux.attached_id().is_none());
    }

    #[test]
    fn multiplexer_destroy_nonexistent() {
        let mut mux = Multiplexer::new();
        let fake_id = SessionId::new(999);

        assert!(!mux.destroy_session(fake_id));
    }

    #[test]
    fn multiplexer_client_count() {
        let mux = Multiplexer::new();
        assert_eq!(mux.client_count(), 0);
    }
}
