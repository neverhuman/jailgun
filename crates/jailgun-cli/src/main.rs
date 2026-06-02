mod agent;
mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = cli::Cli::parse();
    commands::dispatch(cli.command).await
}
