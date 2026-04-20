//! Basilisk - GPU-accelerated terminal emulator

use clap::Parser;
use std::path::PathBuf;

use basilisk::config::Config;
use basilisk::app::App;
use basilisk::Result;

#[derive(Parser, Debug)]
#[command(name = "basilisk")]
#[command(about = "GPU-accelerated terminal emulator")]
#[command(version)]
struct Cli {
    /// Execute command instead of shell
    #[arg(short = 'e', long)]
    execute: Option<String>,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Keep terminal open after command exits
    #[arg(long)]
    hold: bool,

    /// Working directory
    #[arg(short = 'd', long = "working-directory")]
    working_dir: Option<PathBuf>,

    /// Window title
    #[arg(short = 'T', long = "title")]
    title: Option<String>,

    /// Attach to existing session
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Parser, Debug)]
enum Command {
    /// Attach to a session
    Attach {
        /// Session name
        session: Option<String>,
    },
    /// List sessions
    List,
    /// Create a new named session
    New {
        /// Session name
        #[arg(short, long)]
        name: Option<String>,
    },
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    let mut config = match &cli.config {
        Some(path) => Config::load(path)?,
        None => {
            let default_path = Config::default_path();
            if default_path.exists() {
                Config::load(&default_path)?
            } else {
                Config::default()
            }
        }
    };

    // Override shell if -e specified
    // For -e, we wrap the command to handle --hold properly
    if let Some(cmd) = &cli.execute {
        if cli.hold {
            // Wrap command to pause after exit
            #[cfg(unix)]
            {
                // Use shell to run command and then wait for input on exit
                config.terminal.shell = format!(
                    "/bin/sh -c '{}; echo; echo \"[Process exited - press Enter to close]\"; read'",
                    cmd.replace('\'', "'\\''")
                );
            }
            #[cfg(not(unix))]
            {
                // On non-Unix, just set the command directly
                config.terminal.shell = cmd.clone();
            }
        } else {
            // Direct execution - wrap in shell for proper handling
            #[cfg(unix)]
            {
                config.terminal.shell = format!("/bin/sh -c '{}'", cmd.replace('\'', "'\\''"));
            }
            #[cfg(not(unix))]
            {
                config.terminal.shell = cmd.clone();
            }
        }
    }

    // Change working directory if specified
    if let Some(ref dir) = cli.working_dir {
        if dir.exists() {
            if let Err(e) = std::env::set_current_dir(dir) {
                log::warn!("Failed to change working directory: {}", e);
            }
        } else {
            log::warn!("Working directory does not exist: {:?}", dir);
        }
    }

    match cli.command {
        Some(Command::List) => {
            list_sessions()?;
        }
        Some(Command::Attach { session }) => {
            attach_session(session)?;
        }
        Some(Command::New { name }) => {
            // Create new named session
            let session_name = name.unwrap_or_else(|| {
                format!("session-{}", std::process::id())
            });
            log::info!("Creating new session: {}", session_name);
            App::run(config)?;
        }
        None => {
            // Launch terminal
            App::run(config)?;
        }
    }

    Ok(())
}

/// List running sessions
fn list_sessions() -> Result<()> {
    let session_dir = session_directory();
    
    if !session_dir.exists() {
        println!("No sessions running");
        return Ok(());
    }

    let entries: Vec<_> = std::fs::read_dir(&session_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|s| s == "sock").unwrap_or(false))
        .collect();

    if entries.is_empty() {
        println!("No sessions running");
    } else {
        println!("Sessions:");
        for entry in entries {
            let path = entry.path();
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            println!("  {}", name);
        }
    }

    Ok(())
}

/// Attach to an existing session
#[cfg(unix)]
fn attach_session(session: Option<String>) -> Result<()> {
    use std::io::{Read, Write};
    use std::os::unix::io::{AsRawFd, BorrowedFd};
    use nix::sys::termios::{self, LocalFlags, InputFlags, OutputFlags, SetArg};
    use nix::poll::{poll, PollFd, PollFlags, PollTimeout};

    let session_dir = session_directory();
    
    let session_name = match session {
        Some(name) => name,
        None => {
            if !session_dir.exists() {
                return Err(basilisk::Error::Config("No sessions available".into()));
            }

            let first = std::fs::read_dir(&session_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|s| s == "sock").unwrap_or(false))
                .next();

            match first {
                Some(entry) => {
                    entry.path()
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                        .ok_or_else(|| basilisk::Error::Config("Invalid session name".into()))?
                }
                None => {
                    return Err(basilisk::Error::Config("No sessions available".into()));
                }
            }
        }
    };

    let socket_path = session_dir.join(format!("{}.sock", session_name));
    
    if !socket_path.exists() {
        return Err(basilisk::Error::Config(format!("Session '{}' not found", session_name)));
    }

    println!("Attaching to session: {}", session_name);
    
    // Save original terminal settings
    let stdin_fd = std::io::stdin().as_raw_fd();
    let stdin_borrowed = unsafe { BorrowedFd::borrow_raw(stdin_fd) };
    let original_termios = termios::tcgetattr(&stdin_borrowed)
        .map_err(|e| basilisk::Error::Pty(format!("Failed to get termios: {}", e)))?;
    
    // Set raw mode
    let mut raw_termios = original_termios.clone();
    raw_termios.local_flags.remove(
        LocalFlags::ICANON | LocalFlags::ECHO | LocalFlags::ISIG | LocalFlags::IEXTEN
    );
    raw_termios.input_flags.remove(
        InputFlags::IXON | InputFlags::ICRNL | InputFlags::BRKINT | InputFlags::INPCK | InputFlags::ISTRIP
    );
    raw_termios.output_flags.remove(OutputFlags::OPOST);
    
    termios::tcsetattr(&stdin_borrowed, SetArg::TCSANOW, &raw_termios)
        .map_err(|e| basilisk::Error::Pty(format!("Failed to set raw mode: {}", e)))?;

    // Restore terminal on scope exit
    struct TermiosGuard {
        fd: i32,
        termios: nix::sys::termios::Termios,
    }
    impl Drop for TermiosGuard {
        fn drop(&mut self) {
            let stdin_borrowed = unsafe { BorrowedFd::borrow_raw(self.fd) };
            let _ = termios::tcsetattr(&stdin_borrowed, SetArg::TCSANOW, &self.termios);
        }
    }
    let _guard = TermiosGuard { fd: stdin_fd, termios: original_termios };

    // Connect to the session
    let mut client = basilisk::mux::SessionClient::connect(&session_name)?;
    client.send(&basilisk::mux::IpcMessage::Attach)?;
    
    match client.recv()? {
        Some(basilisk::mux::IpcMessage::AttachAck { cols, rows }) => {
            eprintln!("Attached to session ({}x{})", cols, rows);
            
            client.set_nonblocking(true)?;
            
            let mut stdin = std::io::stdin();
            let mut stdout = std::io::stdout();
            let mut input_buf = [0u8; 4096];
            
            // Main I/O loop
            loop {
                // Use poll() to wait for input on stdin
                let stdin_borrowed = unsafe { BorrowedFd::borrow_raw(stdin_fd) };
                let mut poll_fds = [PollFd::new(stdin_borrowed, PollFlags::POLLIN)];
                
                match poll(&mut poll_fds, PollTimeout::from(10u16)) { // 10ms timeout
                    Ok(n) if n > 0 => {
                        // stdin has data
                        if poll_fds[0].revents().map(|r| r.contains(PollFlags::POLLIN)).unwrap_or(false) {
                            match stdin.read(&mut input_buf) {
                                Ok(0) => break, // EOF
                                Ok(n) => {
                                    // Check for detach key (Ctrl+B d)
                                    // For now, just forward all input
                                    client.send(&basilisk::mux::IpcMessage::Input(input_buf[..n].to_vec()))?;
                                }
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                                Err(e) => {
                                    log::warn!("Error reading stdin: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                    Ok(_) => {} // Timeout
                    Err(nix::errno::Errno::EINTR) => continue, // Interrupted, retry
                    Err(e) => {
                        log::error!("poll() error: {}", e);
                        break;
                    }
                }
                
                // Check for output from session (non-blocking)
                match client.recv() {
                    Ok(Some(basilisk::mux::IpcMessage::Output(data))) => {
                        stdout.write_all(&data)?;
                        stdout.flush()?;
                    }
                    Ok(Some(basilisk::mux::IpcMessage::SessionEnd)) => {
                        eprintln!("\r\nSession ended.");
                        break;
                    }
                    Ok(Some(basilisk::mux::IpcMessage::Resize { cols, rows })) => {
                        // Could resize our terminal here
                        log::info!("Session resized to {}x{}", cols, rows);
                    }
                    Ok(_) => {}
                    Err(ref e) if e.to_string().contains("WouldBlock") => {}
                    Err(e) => {
                        log::warn!("Error receiving from session: {}", e);
                    }
                }
            }
        }
        Some(basilisk::mux::IpcMessage::SessionEnd) => {
            println!("Session has ended.");
        }
        _ => {
            return Err(basilisk::Error::Config("Failed to attach to session".into()));
        }
    }

    Ok(())
}

#[cfg(not(unix))]
fn attach_session(_session: Option<String>) -> Result<()> {
    Err(basilisk::Error::Config("Session attach not supported on this platform".into()))
}

/// Get the session directory path
fn session_directory() -> PathBuf {
    dirs::runtime_dir()
        .or_else(|| dirs::cache_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("basilisk")
        .join("sessions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_directory_exists() {
        let dir = session_directory();
        // Just check it returns a reasonable path
        assert!(dir.to_str().unwrap().contains("basilisk"));
    }
}
