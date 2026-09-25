use std::time::Duration;

use jailgun_core::JailgunEvent;
use tokio::sync::{broadcast, mpsc};

use crate::{
    config::RunOptions,
    errors::OrchestratorError,
    run::{bridge_flow::send_tab_commands_for_tab, publish::publish_browser_log},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LaunchTrigger {
    pub(super) tab_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LaunchDelay {
    pub(super) tab_id: u16,
    pub(super) duration: Duration,
    pub(super) reason: &'static str,
}

#[derive(Debug)]
pub(super) struct LaunchScheduler {
    pub(super) total_tabs: u16,
    pub(super) next_tab: u16,
    pub(super) waiting_for_acceptance: Option<u16>,
    pub(super) scheduled_launch: Option<u16>,
}

impl LaunchScheduler {
    pub(super) fn new(total_tabs: u16) -> Self {
        Self {
            total_tabs,
            next_tab: 1,
            waiting_for_acceptance: None,
            scheduled_launch: None,
        }
    }

    pub(super) async fn launch_next(
        &mut self,
        opts: &RunOptions,
        commands: &mpsc::Sender<crate::bridge::Envelope<serde_json::Value>>,
        events: &broadcast::Sender<JailgunEvent>,
    ) -> Result<(), OrchestratorError> {
        if self.next_tab > self.total_tabs {
            return Ok(());
        }
        let tab_id = self.next_tab;
        self.next_tab = self.next_tab.saturating_add(1);
        self.waiting_for_acceptance = Some(tab_id);
        publish_browser_log(
            events,
            &opts.run_id,
            Some(tab_id),
            "launch-tab",
            "started",
            "opening tab and queueing upload, submit, monitor commands",
            [
                ("tab_id", tab_id.to_string()),
                ("total_tabs", self.total_tabs.to_string()),
            ],
        );
        send_tab_commands_for_tab(opts, commands, tab_id).await
    }

    pub(super) fn prompt_accepted(&mut self, tab_id: u16, delay: Duration) -> Option<LaunchDelay> {
        if self.waiting_for_acceptance != Some(tab_id) {
            return None;
        }
        self.waiting_for_acceptance = None;
        self.schedule_next(delay, "prompt-accepted")
    }

    pub(super) fn tab_terminal(&mut self, tab_id: u16, delay: Duration) -> Option<LaunchDelay> {
        if self.waiting_for_acceptance != Some(tab_id) {
            return None;
        }
        self.waiting_for_acceptance = None;
        self.schedule_next(delay, "tab-terminal-before-prompt-accepted")
    }

    pub(super) fn tab_failed(&mut self, tab_id: u16) {
        if self.waiting_for_acceptance == Some(tab_id) {
            self.waiting_for_acceptance = None;
        }
        self.scheduled_launch = None;
    }

    pub(super) fn schedule_next(
        &mut self,
        duration: Duration,
        reason: &'static str,
    ) -> Option<LaunchDelay> {
        if self.next_tab > self.total_tabs || self.scheduled_launch.is_some() {
            return None;
        }
        let tab_id = self.next_tab;
        self.scheduled_launch = Some(tab_id);
        Some(LaunchDelay {
            tab_id,
            duration,
            reason,
        })
    }

    pub(super) fn consume_scheduled_launch(&mut self, tab_id: u16) -> bool {
        if self.scheduled_launch == Some(tab_id) {
            self.scheduled_launch = None;
            true
        } else {
            false
        }
    }
}

pub(super) fn schedule_launch_timer(
    events: &broadcast::Sender<JailgunEvent>,
    run_id: &str,
    launch_tx: &mpsc::Sender<LaunchTrigger>,
    tab_id: u16,
    duration: Duration,
    reason: &'static str,
) {
    publish_browser_log(
        events,
        run_id,
        Some(tab_id),
        "launch-delay",
        "waiting",
        "waiting before launching next tab",
        [
            ("next_tab", tab_id.to_string()),
            ("delay_ms", duration.as_millis().to_string()),
            ("reason", reason.to_string()),
        ],
    );
    let launch_tx = launch_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(duration).await;
        let _ = launch_tx.send(LaunchTrigger { tab_id }).await;
    });
}
