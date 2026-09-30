use super::*;
use crate::concept::{display::ManagedDisplay, LoginView};
use jailgun_workflow::model::AccountSession;

impl AccountSupervisor {
    pub async fn sessions(&self) -> anyhow::Result<Vec<AccountSession>> {
        let mut sessions = self.store.account_sessions().await?;
        let displays = self.displays.lock().await;
        for session in &mut sessions {
            session.login_view_available = login_active(session)
                && match displays.get(&session.account_id) {
                    Some(display) => display.alive().await,
                    None => false,
                };
        }
        Ok(sessions)
    }

    pub async fn login_view_active(&self, id: &str) -> bool {
        self.sessions().await.is_ok_and(|sessions| {
            sessions
                .iter()
                .any(|s| s.account_id == id && s.login_view_available)
        })
    }

    /// Only the authenticated operator WebSocket handler exposes this stream.
    pub async fn open_login_view(&self, id: &str) -> anyhow::Result<LoginView> {
        anyhow::ensure!(self.login_view_active(id).await, "login-view-unavailable");
        let display = self
            .displays
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("login-view-unavailable"))?;
        display.open_view().await
    }

    pub(super) async fn browser_environment(
        &self,
        account: &BrowserAccount,
    ) -> anyhow::Result<BTreeMap<String, String>> {
        let mut environment = self.runtime.environment.clone();
        if self.runtime.server_browser {
            let mut displays = self.displays.lock().await;
            if let Some(display) = displays.get(&account.id) {
                if !display.alive().await {
                    displays.remove(&account.id);
                }
            }
            if !displays.contains_key(&account.id) {
                displays.insert(
                    account.id.clone(),
                    Arc::new(ManagedDisplay::start(&account.state_dir).await?),
                );
            }
            environment.extend(displays[&account.id].environment());
        }
        Ok(environment)
    }
}

fn login_active(session: &AccountSession) -> bool {
    ["login-required", "waiting-for-user", "verifying"].contains(&session.lifecycle.as_str())
        && session
            .login_expires_ms
            .is_some_and(|expires| expires > now())
}
