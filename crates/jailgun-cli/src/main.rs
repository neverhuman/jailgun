use anyhow::Result;
use clap::Parser;
use jailgun_cli::{cli::Cli, commands};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    commands::dispatch(cli.command).await
}
