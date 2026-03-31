//! Basilisk - GPU-accelerated terminal emulator

use clap::Parser;
use std::path::PathBuf;

use basilisk::config::Config;
use basilisk::Result;

#[derive(Parser, Debug)]
#[command(name = "basilisk")]
#[command(about = "GPU-accelerated terminal emulator")]
#[command(version)]
struct Cli {
    /// Execute command instead of shell
    #[arg(short, long)]
    execute: Option<String>,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Keep terminal open after command exits
    #[arg(long)]
    hold: bool,

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
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    let config = match &cli.config {
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

    match cli.command {
        Some(Command::List) => {
            println!("No sessions running");
        }
        Some(Command::Attach { session }) => {
            println!("Attach to session: {:?}", session);
        }
        None => {
            println!("Basilisk terminal emulator");
            println!("Config: {:?}", config.font);
            // TODO: Launch terminal window
        }
    }

    Ok(())
}
