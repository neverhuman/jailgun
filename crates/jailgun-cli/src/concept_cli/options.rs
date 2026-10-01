use crate::commands::concept_daemon::ConceptDaemonOptions;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Default, Args)]
pub struct ConnectionOptions {
    /// Connect to this loopback origin (use SSH forwarding for a remote daemon).
    #[arg(long, global = true, env = "JAILGUN_URL")]
    pub url: Option<String>,
    /// Read an explicit private credential; otherwise use JAILGUN_TOKEN or the local operator credential.
    #[arg(long, global = true, env = "JAILGUN_TOKEN_FILE")]
    pub token_file: Option<PathBuf>,
    /// Report an unavailable daemon without starting it.
    #[arg(long, global = true)]
    pub no_start: bool,
    #[command(flatten)]
    pub daemon: ConceptDaemonOptions,
}

#[derive(Debug, Args)]
pub struct BrainstormOptions {
    #[arg(
        required_unless_present = "concept_file",
        conflicts_with = "concept_file"
    )]
    pub concept: Option<String>,
    #[arg(long)]
    pub concept_file: Option<PathBuf>,
    #[arg(long)]
    pub account: String,
    #[arg(long,default_value_t=5,value_parser=clap::value_parser!(u16).range(5..=10))]
    pub tabs: u16,
    #[arg(long, default_value = "")]
    pub constraints: String,
    /// Repeat NAME=WEIGHT; weights must total 100. Omit to use the default criteria.
    #[arg(long = "criterion", value_name = "NAME=WEIGHT")]
    pub criteria: Vec<String>,
    /// Reuse this key only when retrying the identical request.
    #[arg(long)]
    pub idempotency_key: Option<String>,
    #[arg(long)]
    pub wait: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

#[derive(Debug, Subcommand)]
pub enum AccountsCommand {
    List,
    /// Open account onboarding. An email optionally starts registration immediately.
    Connect {
        #[arg(long)]
        email: Option<String>,
        #[arg(long, requires = "email")]
        id: Option<String>,
        #[arg(long)]
        no_open: bool,
    },
    Reconnect {
        account_id: String,
        #[arg(long)]
        no_open: bool,
    },
}
#[derive(Debug, Subcommand)]
pub enum RunsCommand {
    List,
    Show {
        run_id: String,
    },
    Result {
        run_id: String,
    },
    Pause {
        run_id: String,
    },
    Resume {
        run_id: String,
        #[arg(long)]
        allow_incomplete: bool,
    },
    Cancel {
        run_id: String,
    },
    Export {
        run_id: String,
        #[arg(long)]
        out: PathBuf,
    },
}
