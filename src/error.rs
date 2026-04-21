use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("PTY error: {0}")]
    Pty(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Render error: {0}")]
    Render(String),

    #[error("Font error: {0}")]
    Font(String),

    #[error("Window error: {0}")]
    Window(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Clipboard error: {0}")]
    Clipboard(String),

    #[error("SSH error: {0}")]
    Ssh(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = Error::Pty("spawn failed".into());
        assert_eq!(err.to_string(), "PTY error: spawn failed");
    }

    #[test]
    fn error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }
}
