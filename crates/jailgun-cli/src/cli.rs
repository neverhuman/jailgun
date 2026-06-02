use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use jailgun_core::CleanupPolicy;

#[derive(Debug, Parser)]
#[command(name = "jailgun")]
#[command(about = "Rust core for ChatGPT archive capture and safe deploy")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    ValidateConfig {
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
    },
    TarValidate {
        archive: PathBuf,
        #[arg(long)]
        require_single_top_level: bool,
    },
    Scan {
        paths: Vec<PathBuf>,
    },
    RemoteCleanup {
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        tab_id: Option<u16>,
        #[arg(long)]
        remote_host: Option<String>,
        #[arg(long)]
        remote_dir: Option<String>,
        #[arg(long)]
        receipt_dir: Option<PathBuf>,
        #[arg(long, value_enum)]
        policy: Option<CleanupPolicyArg>,
    },
    DeployArchive {
        archive: PathBuf,
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
        #[arg(long)]
        run_id: String,
        #[arg(long, default_value_t = 1)]
        tab_id: u16,
        #[arg(long)]
        remote_host: Option<String>,
        #[arg(long)]
        remote_dir: Option<String>,
        #[arg(long)]
        remote_command: Option<String>,
        #[arg(long)]
        receipt_dir: Option<PathBuf>,
        #[arg(long, value_enum)]
        policy: Option<CleanupPolicyArg>,
        #[arg(long)]
        dry_run: bool,
        /// Refuse deploy when the archive's single top-level directory is not this value.
        #[arg(long, default_value = "jekko")]
        expected_top_level: String,
        #[arg(long, default_value_t = 360)]
        status_max_minutes: u16,
        #[arg(long)]
        ci: bool,
        #[arg(long)]
        ci_repo: Option<String>,
    },
    Run {
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
        #[arg(long)]
        prompt_file: PathBuf,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        tabs: Option<u16>,
        #[arg(long)]
        source_repo_url: Option<String>,
        #[arg(long)]
        source_ref: Option<String>,
        #[arg(long)]
        deploy: bool,
        #[arg(long)]
        no_deploy: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        remote_host: Option<String>,
        #[arg(long)]
        remote_dir: Option<String>,
        #[arg(long)]
        remote_command: Option<String>,
        #[arg(long)]
        expected_top_level: Option<String>,
        #[arg(long)]
        tar_target_name: Option<String>,
        #[arg(long)]
        profile_dir: Option<PathBuf>,
        #[arg(long)]
        downloads_dir: Option<PathBuf>,
        #[arg(long)]
        artifacts_dir: Option<PathBuf>,
        #[arg(long, num_args = 1.., value_name = "ARG", allow_hyphen_values = true)]
        bridge_cmd: Vec<String>,
        #[arg(long = "bridge-env", value_name = "KEY=VALUE")]
        bridge_env: Vec<String>,
        #[arg(long, default_value_t = 1024)]
        event_buffer: usize,
        #[arg(long, default_value_t = 1)]
        deploy_concurrency: u16,
        #[arg(long, default_value_t = 360)]
        status_max_minutes: u16,
        #[arg(long)]
        ci: bool,
        #[arg(long)]
        ci_repo: Option<String>,
        #[arg(long, default_value = "main")]
        ci_branch: String,
        #[arg(long, default_value_t = 20)]
        ci_max_attempts: u32,
        #[arg(long, default_value_t = 30)]
        ci_poll_seconds: u16,
        #[arg(long)]
        notify_telegram: bool,
        #[arg(long, default_value = "telegram/token.env")]
        telegram_token_file: PathBuf,
        #[arg(long, default_value = "telegram/chat_id.cache")]
        telegram_chat_id_cache: PathBuf,
    },
    RunAgent {
        #[arg(long)]
        request: String,
        #[arg(long)]
        events_jsonl: PathBuf,
        #[arg(long)]
        summary_json: PathBuf,
    },
    ReviewPacket {
        #[arg(long = "summary-json")]
        summary_json: PathBuf,
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value_t = 128 * 1024)]
        patch_bytes: usize,
    },
    TelegramSend {
        #[arg(long, default_value = "telegram/token.env")]
        token_file: PathBuf,
        #[arg(long, default_value = "telegram/chat_id.cache")]
        chat_id_cache: PathBuf,
        #[arg(long)]
        chat_id: Option<String>,
        #[arg(long)]
        message: String,
    },
    NotifyCommit {
        #[arg(long, default_value = "telegram/token.env")]
        token_file: PathBuf,
        #[arg(long, default_value = "telegram/chat_id.cache")]
        chat_id_cache: PathBuf,
        #[arg(long)]
        chat_id: Option<String>,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "HEAD")]
        revision: String,
    },
    Serve {
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
        #[arg(long, default_value = "127.0.0.1:8787")]
        addr: SocketAddr,
        #[arg(long)]
        dashboard_dist: Option<PathBuf>,
        /// Start with a live broadcast bus (AppState::live). The /ws/events
        /// endpoint streams events forwarded via POST /api/events instead of
        /// replaying the fixture once.
        #[arg(long)]
        live: bool,
        /// Required for POST /api/events. When unset, the endpoint returns 503.
        #[arg(long, env = "JAILGUN_INGEST_TOKEN")]
        ingest_token: Option<String>,
        /// Spawn a Telegram subscriber on the live broadcast that pings the
        /// configured bot for three milestones: job started on a tab, tar
        /// acquired, and deploy success with CI passed (or any failure).
        #[arg(long)]
        notify_telegram: bool,
        #[arg(long, default_value = "telegram/token.env")]
        telegram_token_file: PathBuf,
        #[arg(long, default_value = "telegram/chat_id.cache")]
        telegram_chat_id_cache: PathBuf,
    },
    Fixture {
        #[arg(value_enum)]
        kind: FixtureKind,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum FixtureKind {
    Runs,
    Config,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum CleanupPolicyArg {
    Block,
    PreserveReset,
    Adopt,
}

impl From<CleanupPolicyArg> for CleanupPolicy {
    fn from(value: CleanupPolicyArg) -> Self {
        match value {
            CleanupPolicyArg::Block => CleanupPolicy::Block,
            CleanupPolicyArg::PreserveReset => CleanupPolicy::PreserveReset,
            CleanupPolicyArg::Adopt => CleanupPolicy::Adopt,
        }
    }
}
