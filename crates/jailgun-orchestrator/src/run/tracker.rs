use std::collections::BTreeSet;

#[derive(Debug)]
pub(super) struct RunTracker {
    total_tabs: u16,
    deploy_required: bool,
    downloaded_tabs: BTreeSet<u16>,
    deployed_tabs: BTreeSet<u16>,
    terminal_tabs: BTreeSet<u16>,
    session_expired_tabs: BTreeSet<u16>,
}

impl RunTracker {
    pub(super) fn new(total_tabs: u16, deploy_required: bool) -> Self {
        Self {
            total_tabs,
            deploy_required,
            downloaded_tabs: BTreeSet::new(),
            deployed_tabs: BTreeSet::new(),
            terminal_tabs: BTreeSet::new(),
            session_expired_tabs: BTreeSet::new(),
        }
    }

    pub(super) fn mark_downloaded(&mut self, tab_id: u16) {
        self.downloaded_tabs.insert(tab_id);
    }

    pub(super) fn mark_deployed(&mut self, tab_id: u16) {
        self.deployed_tabs.insert(tab_id);
    }

    pub(super) fn mark_terminal(&mut self, tab_id: u16) {
        self.terminal_tabs.insert(tab_id);
    }

    pub(super) fn mark_session_expired(&mut self, tab_id: u16) {
        self.session_expired_tabs.insert(tab_id);
    }

    pub(super) fn tab_session_expired(&self, tab_id: u16) -> bool {
        self.session_expired_tabs.contains(&tab_id)
    }

    pub(super) fn downloaded_count(&self) -> u16 {
        self.downloaded_tabs.len().min(u16::MAX as usize) as u16
    }

    pub(super) fn deployed_count(&self) -> u16 {
        self.deployed_tabs.len().min(u16::MAX as usize) as u16
    }

    pub(super) fn tab_is_complete(&self, tab_id: u16) -> bool {
        if self.terminal_tabs.contains(&tab_id) {
            return true;
        }
        if !self.downloaded_tabs.contains(&tab_id) {
            return false;
        }
        !self.deploy_required || self.deployed_tabs.contains(&tab_id)
    }

    pub(super) fn is_complete(&self) -> bool {
        (1..=self.total_tabs).all(|tab_id| self.tab_is_complete(tab_id))
    }
}

pub(super) fn run_is_complete(tracker: &RunTracker) -> bool {
    tracker.is_complete()
}
