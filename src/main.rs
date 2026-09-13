use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod outline;

#[derive(Parser)]
#[command(
    name = "rust-ai-lean",
    version,
    about = "Token-lean Rust tooling for AI coding agents"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print item signatures with line ranges, without bodies
    Outline {
        /// Also print the first doc-comment line of each item
        #[arg(long)]
        docs: bool,
        /// Files or directories
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Outline { docs, paths } => outline::run(&paths, docs),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}
