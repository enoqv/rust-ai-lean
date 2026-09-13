use clap::Parser;

#[derive(Parser)]
#[command(
    name = "rust-ai-lean",
    version,
    about = "Token-lean Rust tooling for AI coding agents"
)]
struct Cli {}

fn main() {
    Cli::parse();
}
