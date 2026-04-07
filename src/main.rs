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
fn attach_session(session: Option<String>) -> Result<()> {
    let session_dir = session_directory();
    
    let session_name = match session {
        Some(name) => name,
        None => {
            // Find first available session
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
    
    // Connect to the session via Unix socket
    let mut client = basilisk::mux::SessionClient::connect(&session_name)?;
    
    // Send attach request
    client.send(&basilisk::mux::IpcMessage::Attach)?;
    
    // Wait for acknowledgment
    match client.recv()? {
        Some(basilisk::mux::IpcMessage::AttachAck { cols, rows }) => {
            println!("Attached to session ({}x{})", cols, rows);
            
            // Enter raw mode and forward I/O
            // This is a simplified implementation - full version would need
            // proper terminal raw mode and signal handling
            use std::io::{Read, Write};
            
            client.set_nonblocking(true)?;
            
            let stdin = std::io::stdin();
            let mut stdout = std::io::stdout();
            let mut input_buf = [0u8; 1024];
            
            loop {
                // Check for input from stdin
                // Note: This is simplified - real impl needs termios raw mode
                
                // Check for output from session
                match client.recv() {
                    Ok(Some(basilisk::mux::IpcMessage::Output(data))) => {
                        stdout.write_all(&data)?;
                        stdout.flush()?;
                    }
                    Ok(Some(basilisk::mux::IpcMessage::SessionEnd)) => {
                        println!("\nSession ended.");
                        break;
                    }
                    Ok(_) => {}
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
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
