//! Session IPC for attach/detach functionality
//!
//! Uses Unix domain sockets for client-server communication.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use crate::{Error, Result};

/// IPC message types
#[derive(Debug, Clone)]
pub enum IpcMessage {
    /// Client wants to attach to session
    Attach,
    /// Server acknowledges attach
    AttachAck { cols: u16, rows: u16 },
    /// Client is detaching
    Detach,
    /// Terminal output data
    Output(Vec<u8>),
    /// Terminal input data
    Input(Vec<u8>),
    /// Resize request
    Resize { cols: u16, rows: u16 },
    /// Session ended
    SessionEnd,
}

impl IpcMessage {
    /// Serialize message to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            IpcMessage::Attach => {
                buf.push(0x01);
            }
            IpcMessage::AttachAck { cols, rows } => {
                buf.push(0x02);
                buf.extend_from_slice(&cols.to_le_bytes());
                buf.extend_from_slice(&rows.to_le_bytes());
            }
            IpcMessage::Detach => {
                buf.push(0x03);
            }
            IpcMessage::Output(data) => {
                buf.push(0x04);
                buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
                buf.extend_from_slice(data);
            }
            IpcMessage::Input(data) => {
                buf.push(0x05);
                buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
                buf.extend_from_slice(data);
            }
            IpcMessage::Resize { cols, rows } => {
                buf.push(0x06);
                buf.extend_from_slice(&cols.to_le_bytes());
                buf.extend_from_slice(&rows.to_le_bytes());
            }
            IpcMessage::SessionEnd => {
                buf.push(0x07);
            }
        }
        buf
    }

    /// Deserialize message from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        match data[0] {
            0x01 => Some(IpcMessage::Attach),
            0x02 if data.len() >= 5 => {
                let cols = u16::from_le_bytes([data[1], data[2]]);
                let rows = u16::from_le_bytes([data[3], data[4]]);
                Some(IpcMessage::AttachAck { cols, rows })
            }
            0x03 => Some(IpcMessage::Detach),
            0x04 if data.len() >= 5 => {
                let len = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;
                if data.len() >= 5 + len {
                    Some(IpcMessage::Output(data[5..5 + len].to_vec()))
                } else {
                    None
                }
            }
            0x05 if data.len() >= 5 => {
                let len = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;
                if data.len() >= 5 + len {
                    Some(IpcMessage::Input(data[5..5 + len].to_vec()))
                } else {
                    None
                }
            }
            0x06 if data.len() >= 5 => {
                let cols = u16::from_le_bytes([data[1], data[2]]);
                let rows = u16::from_le_bytes([data[3], data[4]]);
                Some(IpcMessage::Resize { cols, rows })
            }
            0x07 => Some(IpcMessage::SessionEnd),
            _ => None,
        }
    }
}

/// Session server - runs in the background, accepts client connections
pub struct SessionServer {
    socket_path: PathBuf,
    listener: UnixListener,
}

impl SessionServer {
    /// Create a new session server
    pub fn new(session_name: &str) -> Result<Self> {
        let socket_path = session_socket_path(session_name);

        // Ensure parent directory exists
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove stale socket
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path)
            .map_err(|e| Error::Io(e))?;

        Ok(Self {
            socket_path,
            listener,
        })
    }

    /// Accept a client connection (blocking)
    pub fn accept(&self) -> Result<UnixStream> {
        let (stream, _) = self.listener.accept()
            .map_err(|e| Error::Io(e))?;
        Ok(stream)
    }

    /// Accept a client connection as SessionClient (blocking)
    pub fn accept_client(&self) -> Result<SessionClient> {
        let stream = self.accept()?;
        Ok(SessionClient { stream })
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Set non-blocking mode
    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
        self.listener.set_nonblocking(nonblocking)
            .map_err(|e| Error::Io(e))
    }
}

impl Drop for SessionServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Session client - connects to an existing session
pub struct SessionClient {
    stream: UnixStream,
}

impl SessionClient {
    /// Connect to an existing session
    pub fn connect(session_name: &str) -> Result<Self> {
        let socket_path = session_socket_path(session_name);

        if !socket_path.exists() {
            return Err(Error::Config(format!("Session '{}' not found", session_name)));
        }

        let stream = UnixStream::connect(&socket_path)
            .map_err(|e| Error::Io(e))?;

        Ok(Self { stream })
    }

    /// Send a message
    pub fn send(&mut self, msg: &IpcMessage) -> Result<()> {
        let data = msg.to_bytes();
        let len = data.len() as u32;
        self.stream.write_all(&len.to_le_bytes())?;
        self.stream.write_all(&data)?;
        self.stream.flush()?;
        Ok(())
    }

    /// Receive a message (blocking)
    pub fn recv(&mut self) -> Result<Option<IpcMessage>> {
        let mut len_buf = [0u8; 4];
        match self.stream.read_exact(&mut len_buf) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(Error::Io(e)),
        }

        let len = u32::from_le_bytes(len_buf) as usize;
        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data)?;

        Ok(IpcMessage::from_bytes(&data))
    }

    /// Set non-blocking mode
    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
        self.stream.set_nonblocking(nonblocking)
            .map_err(|e| Error::Io(e))
    }
}

/// Get the socket path for a session
pub fn session_socket_path(name: &str) -> PathBuf {
    let runtime_dir = dirs::runtime_dir()
        .or_else(|| dirs::cache_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    runtime_dir
        .join("basilisk")
        .join("sessions")
        .join(format!("{}.sock", name))
}

/// List available sessions
pub fn list_sessions() -> Vec<String> {
    let session_dir = dirs::runtime_dir()
        .or_else(|| dirs::cache_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("basilisk")
        .join("sessions");

    if !session_dir.exists() {
        return Vec::new();
    }

    std::fs::read_dir(&session_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|s| s == "sock").unwrap_or(false))
                .filter_map(|e| {
                    e.path()
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_roundtrip_attach() {
        let msg = IpcMessage::Attach;
        let bytes = msg.to_bytes();
        let decoded = IpcMessage::from_bytes(&bytes);
        assert!(matches!(decoded, Some(IpcMessage::Attach)));
    }

    #[test]
    fn message_roundtrip_attach_ack() {
        let msg = IpcMessage::AttachAck { cols: 80, rows: 24 };
        let bytes = msg.to_bytes();
        let decoded = IpcMessage::from_bytes(&bytes);
        assert!(matches!(decoded, Some(IpcMessage::AttachAck { cols: 80, rows: 24 })));
    }

    #[test]
    fn message_roundtrip_output() {
        let msg = IpcMessage::Output(b"Hello".to_vec());
        let bytes = msg.to_bytes();
        let decoded = IpcMessage::from_bytes(&bytes);
        match decoded {
            Some(IpcMessage::Output(data)) => assert_eq!(data, b"Hello"),
            _ => panic!("Expected Output message"),
        }
    }

    #[test]
    fn session_socket_path_format() {
        let path = session_socket_path("test");
        assert!(path.to_str().unwrap().contains("basilisk"));
        assert!(path.to_str().unwrap().ends_with("test.sock"));
    }
}
