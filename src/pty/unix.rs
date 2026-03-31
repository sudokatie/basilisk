//! Unix PTY implementation

use std::os::unix::io::{AsRawFd, RawFd, FromRawFd, IntoRawFd};
use nix::pty::{openpty, Winsize};
use nix::unistd::{fork, ForkResult, dup2, setsid};
use nix::libc;
use std::ffi::CString;
use std::io::{Read, Write};
use std::fs::File;

use crate::{Error, Result};

/// Pseudo-terminal handle
pub struct Pty {
    master: File,
    pid: i32,
}

impl Pty {
    /// Spawn a shell in a new PTY
    pub fn spawn(shell: &str, cols: u16, rows: u16) -> Result<Self> {
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        let pty = openpty(Some(&winsize), None)
            .map_err(|e| Error::Pty(e.to_string()))?;

        let master_fd = pty.master.as_raw_fd();
        let slave_fd = pty.slave.as_raw_fd();

        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                // Child process - drop master
                drop(pty.master);
                setsid().ok();

                // Set up slave as controlling terminal
                dup2(slave_fd, 0).ok();
                dup2(slave_fd, 1).ok();
                dup2(slave_fd, 2).ok();

                if slave_fd > 2 {
                    drop(pty.slave);
                }

                // Execute shell
                let shell_cstr = CString::new(shell).unwrap();
                let args: [CString; 1] = [shell_cstr.clone()];
                nix::unistd::execvp(&shell_cstr, &args).ok();

                // If exec fails, exit
                std::process::exit(1);
            }
            Ok(ForkResult::Parent { child }) => {
                // Parent - close slave, keep master
                drop(pty.slave);

                // Convert OwnedFd to File for easier read/write
                let master_file = unsafe { File::from_raw_fd(pty.master.into_raw_fd()) };

                Ok(Self {
                    master: master_file,
                    pid: child.as_raw(),
                })
            }
            Err(e) => Err(Error::Pty(e.to_string())),
        }
    }

    /// Read from PTY
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.master.read(buf).map_err(Error::Io)
    }

    /// Write to PTY
    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.master.write(buf).map_err(Error::Io)
    }

    /// Resize PTY
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        unsafe {
            if libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &winsize) < 0 {
                return Err(Error::Pty("ioctl TIOCSWINSZ failed".into()));
            }
        }
        Ok(())
    }

    /// Get master file descriptor
    pub fn master_fd(&self) -> RawFd {
        self.master.as_raw_fd()
    }

    /// Get child PID
    pub fn pid(&self) -> i32 {
        self.pid
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // File handle closes automatically
        // Signal child to terminate
        unsafe {
            libc::kill(self.pid, libc::SIGTERM);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_spawn() {
        // This test requires a real shell, skip in CI
        if std::env::var("CI").is_ok() {
            return;
        }

        let pty = Pty::spawn("/bin/sh", 80, 24);
        assert!(pty.is_ok());
    }
}
