pub mod concept_daemon;
pub mod data;
mod deploy;
pub mod mcp;
mod review;
mod run;
mod server;
mod telegram;
mod uninstall;
mod validate;

use anyhow::Result;

use crate::{
    agent, auth,
    cli::{AuthCommand, Command},
    commands::{
        deploy::{deploy_archive, remote_cleanup},
        review::review_packet,
        run::run,
        server::{fixture, serve},
        telegram::{notify_commit, telegram_send},
        validate::{scan, tar_validate, validate_config},
    },
};

pub async fn dispatch(command: Command) -> Result<()> {
    anyhow::ensure!(
        dispatch_with_output(command, false).await?,
        "command checks require attention"
    );
    Ok(())
}

pub async fn dispatch_with_output(command: Command, json: bool) -> Result<bool> {
    if let Command::Uninstall { prefix } = command {
        return uninstall::run(prefix, json);
    }
    let _application_use = jailgun_core::managed_installation::current_use()
        .map_err(crate::concept_cli::error::installation)?;
    match command {
        Command::Worker { options, addr } => concept_daemon::serve(options, addr).await,
        Command::Uninstall { .. } => unreachable!("uninstall is dispatched before acquiring shared application use"),
        Command::Setup { options, no_open } => {
            return crate::concept_cli::setup(options, no_open, json).await
        }
        Command::Doctor { options } => return crate::concept_cli::doctor(options, json).await,
        Command::Data { command } => return data::run(command, json).await,
        Command::Service { options, command } => return crate::concept_cli::service::run(options, command, json).await,
        Command::Brainstorm(options) => return crate::concept_cli::brainstorm(options, json).await,
        Command::Accounts { options, command } => {
            return crate::concept_cli::accounts(options, command, json).await
        }
        Command::Runs { options, command } => {
            return crate::concept_cli::runs(options, command, json).await
        }
        Command::Mcp(options) => mcp::run(options).await,
        Command::ValidateConfig { config } => validate_config(config).await,
        Command::TarValidate {
            archive,
            require_single_top_level,
        } => tar_validate(archive, require_single_top_level).await,
        Command::Scan { paths } => scan(paths).await,
        Command::RemoteCleanup {
            config,
            run_id,
            tab_id,
            remote_host,
            remote_dir,
            receipt_dir,
            policy,
        } => {
            remote_cleanup(
                config,
                run_id,
                tab_id,
                remote_host,
                remote_dir,
                receipt_dir,
                policy,
            )
            .await
        }
        Command::DeployArchive {
            archive,
            config,
            run_id,
            tab_id,
            remote_host,
            remote_dir,
            remote_command,
            receipt_dir,
            policy,
            dry_run,
            expected_top_level,
            status_max_minutes,
            ci,
            ci_repo,
        } => {
            deploy_archive(
                archive,
                config,
                run_id,
                tab_id,
                remote_host,
                remote_dir,
                remote_command,
                receipt_dir,
                policy,
                dry_run,
                expected_top_level,
                status_max_minutes,
                ci,
                ci_repo,
            )
            .await
        }
        Command::Auth { command } => match command {
            AuthCommand::Setup {
                emails,
                id,
                registry,
                profile_root,
                state_root,
                downloads_root,
                cdp_port_start,
                prefer_email_code,
                code_stdin,
                status_watch,
                bridge_cmd,
                bridge_env,
            } => {
                auth::setup(auth::AuthSetupOptions {
                    emails,
                    id,
                    registry,
                    profile_root,
                    state_root,
                    downloads_root,
                    cdp_port_start,
                    prefer_email_code,
                    code_stdin,
                    status_watch,
                    bridge_cmd,
                    bridge_env,
                })
                .await
            }
        },
        Command::Run {
            config,
            prompt_file,
            run_id,
            tabs,
            source_repo_url,
            source_ref,
            deploy,
            no_deploy,
            dry_run,
            remote_host,
            remote_dir,
            remote_command,
            expected_top_level,
            tar_target_name,
            profile_dir,
            downloads_dir,
            artifacts_dir,
            bridge_cmd,
            bridge_env,
            event_buffer,
            deploy_concurrency,
            status_max_minutes,
            ci,
            ci_repo,
            ci_branch,
            ci_max_attempts,
            ci_poll_seconds,
            notify_telegram,
            telegram_token_file,
            telegram_chat_id_cache,
        } => {
            run(
                config,
                prompt_file,
                run_id,
                tabs,
                source_repo_url,
                source_ref,
                deploy,
                no_deploy,
                dry_run,
                remote_host,
                remote_dir,
                remote_command,
                expected_top_level,
                tar_target_name,
                profile_dir,
                downloads_dir,
                artifacts_dir,
                bridge_cmd,
                bridge_env,
                event_buffer,
                deploy_concurrency,
                status_max_minutes,
                ci,
                ci_repo,
                ci_branch,
                ci_max_attempts,
                ci_poll_seconds,
                notify_telegram,
                telegram_token_file,
                telegram_chat_id_cache,
            )
            .await
        }
        Command::RunAgent {
            request,
            events_jsonl,
            summary_json,
        } => agent::run_agent(request, events_jsonl, summary_json).await,
        Command::ReviewPacket {
            summary_json,
            base,
            head,
            repo,
            output,
            patch_bytes,
        } => review_packet(summary_json, base, head, repo, output, patch_bytes).await,
        Command::TelegramSend {
            token_file,
            chat_id_cache,
            chat_id,
            message,
        } => telegram_send(token_file, chat_id_cache, chat_id, message).await,
        Command::NotifyCommit {
            token_file,
            chat_id_cache,
            chat_id,
            repo,
            revision,
        } => notify_commit(token_file, chat_id_cache, chat_id, repo, revision).await,
        Command::Jailhard(args) => crate::jailhard::run(args).await,
        Command::Serve {
            advanced,
            concepts: _,
            concept_options,
            config,
            addr,
            dashboard_dist,
            live,
            ingest_token,
            notify_telegram,
            telegram_token_file,
            telegram_chat_id_cache,
        } => {
            if !advanced && !live {
                return concept_daemon::serve(concept_options, addr).await.map(|()| true);
            }
            if concept_options.runtime.is_some() || concept_options.assets.is_some()
                || concept_options.node.is_some() || concept_options.chrome.is_some() || concept_options.headless || concept_options.server_browser {
                return Err(crate::concept_cli::error::failure("serve-mode-conflict", "Concept runtime options cannot configure the advanced service.", "Use jailgun serve for concepts, or configure the advanced service through --config."));
            }
            serve(
                config.unwrap_or_else(|| "config/jailgun.example.toml".into()),
                addr,
                dashboard_dist,
                live,
                ingest_token,
                notify_telegram,
                telegram_token_file.unwrap_or_else(|| "telegram/token.env".into()),
                telegram_chat_id_cache.unwrap_or_else(|| "telegram/chat_id.cache".into()),
            )
            .await
        }
        Command::Fixture { kind } => fixture(kind).await,
    }
    .map(|()| true)
}

#[cfg(test)]
mod tests;
