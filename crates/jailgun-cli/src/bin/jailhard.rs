use anyhow::Result;
use clap::Parser;
use jailgun_cli::jailhard::JailhardArgs;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    jailgun_cli::jailhard::run(JailhardArgs::parse()).await
}
