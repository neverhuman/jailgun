use clap::Parser;
use jailgun_cli::{cli::Cli, commands, concept_cli::error};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(cause) => {
            if !cause.use_stderr() {
                let _ = cause.print();
                return ExitCode::SUCCESS;
            }
            if std::env::args_os().any(|arg| arg == "--json") {
                let failure = error::failure(
                    "invalid-arguments",
                    cause.to_string(),
                    "Use jailgun --help or the command's --help.",
                );
                eprintln!(
                    "{}",
                    serde_json::to_string(&error::response(&failure)).expect("error JSON")
                );
            } else {
                let _ = cause.print();
            }
            return ExitCode::from(2);
        }
    };
    match commands::dispatch_with_output(cli.command, cli.json).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(cause) => {
            let response = error::response(&cause);
            if cli.json {
                eprintln!("{}", serde_json::to_string(&response).expect("error JSON"));
            } else {
                eprintln!(
                    "{}: {}\n{}",
                    response.code, response.message, response.next_action
                );
            }
            ExitCode::FAILURE
        }
    }
}
