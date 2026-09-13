use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod crate_src;
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
    /// Locate a dependency's source for the exact version in Cargo.lock
    CrateSrc {
        #[arg(value_name = "CRATE")]
        krate: String,
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        /// Print only crate root paths, one per line
        #[arg(long)]
        path_only: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Outline { docs, paths } => outline::run(&paths, docs),
        Cmd::CrateSrc {
            krate,
            manifest_path,
            path_only,
        } => crate_src::run(&krate, manifest_path.as_deref(), path_only),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}
