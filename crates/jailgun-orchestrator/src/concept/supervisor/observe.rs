use super::{now, AccountSupervisor, ConceptBridge};
use crate::bridge::BridgeSpawnConfig;
use jailgun_core::{BrowserAccount, ProviderIdentity};
use jailgun_workflow::model::AccountObservation;
use serde_json::json;
use std::time::Duration;

impl AccountSupervisor {
    // Only account lifecycle operations take this gate. Conversation capture and
    // scheduling keep their own ownership and remain independent of this probe.
    pub(super) async fn watch(&self, account: BrowserAccount) {
        let control = self.account_control(&account.id).await;
        let mut bridge = None;
        loop {
            let delay = {
                let _guard = control.lock().await;
                match self.probe_account(&account, &mut bridge, false).await {
                    Ok(delay) => delay,
                    Err(error) => {
                        tracing::error!(account_id=%account.id, "account supervisor stopped; reconnect through the dashboard");
                        let code = error
                            .downcast_ref::<super::super::BrowserFailure>()
                            .map_or("browser-failed", |error| error.code.as_str());
                        let _ = self.store.account_expired(account.id.clone(), now()).await;
                        let _ = self
                            .store
                            .login_state(
                                account.id.clone(),
                                "failed".into(),
                                Some(code.into()),
                                None,
                            )
                            .await;
                        return;
                    }
                }
            };
            tokio::time::sleep(delay).await;
        }
    }

    /// Recheck a bound browser without navigating it or authenticating on behalf
    /// of the operator. A transient failed probe can recover; cancellation and
    /// identity mismatches remain blocked until explicit dashboard reconnect.
    pub async fn refresh_account(&self, id: &str) -> anyhow::Result<()> {
        let account = self
            .store
            .account_metadata()
            .await?
            .into_iter()
            .find(|account| account.id == id)
            .ok_or_else(|| anyhow::anyhow!("account-not-found"))?;
        let control = self.account_control(id).await;
        let _guard = control.lock().await;
        let mut bridge = self.bridges.lock().await.get(id).cloned();
        self.probe_account(&account, &mut bridge, true).await?;
        Ok(())
    }

    async fn managed_bridge(&self, account: &BrowserAccount) -> anyhow::Result<ConceptBridge> {
        let id = account.id.clone();
        let mut existing = self.bridges.lock().await.get(&id).cloned();
        if let Some(bridge) = &existing {
            if !bridge.is_alive().await {
                self.bridges.lock().await.remove(&id);
                existing = None;
            }
        }
        let bridge = if let Some(bridge) = existing {
            bridge
        } else {
            let identity = account
                .provider_account_id
                .as_ref()
                .map(|provider| ProviderIdentity {
                    id: provider.clone(),
                    email: account.email_hint.clone(),
                });
            let environment = self.browser_environment(account).await?;
            let bridge = ConceptBridge::spawn(BridgeSpawnConfig {command:vec![self.runtime.node.to_string_lossy().into(),self.runtime.bridge.to_string_lossy().into()], env:environment}, json!({"profile_dir":account.profile_dir,"identity":identity,"headless":self.runtime.headless,"executable":self.runtime.chrome,"base_url":self.runtime.provider_url,"cdp_port":account.cdp_port})).await?;
            self.bridges.lock().await.insert(id.clone(), bridge.clone());
            bridge
        };
        Ok(bridge)
    }

    async fn probe_account(
        &self,
        account: &BrowserAccount,
        bridge: &mut Option<ConceptBridge>,
        refresh: bool,
    ) -> anyhow::Result<Duration> {
        let id = account.id.clone();
        let session = self
            .store
            .account_sessions()
            .await?
            .into_iter()
            .find(|s| s.account_id == id)
            .ok_or_else(|| anyhow::anyhow!("account-not-found"))?;
        let retry_expired = refresh
            && session.lifecycle == "expired"
            && session.error_code.as_deref() == Some("authentication-expired")
            && session.model.is_some();
        if ["cancelled", "failed", "mismatch"].contains(&session.lifecycle.as_str())
            || (session.lifecycle == "expired" && !retry_expired)
        {
            return Ok(Duration::from_secs(2));
        }
        if bridge.is_none() {
            *bridge = Some(self.managed_bridge(account).await?);
        }
        let bridge = bridge.as_ref().expect("initialized account bridge");
        if session.lifecycle == "starting" {
            self.store
                .login_state(
                    id.clone(),
                    "login-required".into(),
                    None,
                    session.login_expires_ms,
                )
                .await?;
        }
        if session
            .login_expires_ms
            .is_some_and(|expiry| now() >= expiry)
        {
            self.store
                .login_state(
                    id.clone(),
                    "expired".into(),
                    Some("login-session-expired".into()),
                    None,
                )
                .await?;
            return Ok(Duration::from_secs(2));
        }
        let readiness = self
            .store
            .accounts()
            .await?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("account-not-found"))?;
        if readiness.cooldown_until_ms > now()
            || (readiness.readiness == "paused"
                && session.lifecycle != "starting"
                && session.lifecycle != "verifying")
        {
            return Ok(Duration::from_secs(5));
        }
        // Reconnect clears the observation even when it reuses this watcher.
        // Cache the model list only within that observed login session.
        let discover_models = session.observation.is_none();
        let observation = bridge
            .call(
                "concept.auth-status",
                "account",
                json!({"discover_models":discover_models}),
                Duration::from_secs(20),
            )
            .await;
        match observation {
            Ok(value) => {
                let mut observation: AccountObservation = serde_json::from_value(value)?;
                if !discover_models {
                    if let Some(previous) = session.observation {
                        observation.available_models = previous.available_models;
                    }
                }
                let bound = match self
                    .store
                    .observe_account(id.clone(), observation.clone(), now())
                    .await
                {
                    Ok(bound) => bound,
                    Err(error) if error.code() == "account-mismatch" => {
                        self.store.account_expired(id.clone(), now()).await?;
                        self.store
                            .login_state(
                                id.clone(),
                                "mismatch".into(),
                                Some("account-mismatch".into()),
                                None,
                            )
                            .await?;
                        return Ok(Duration::from_secs(2));
                    }
                    Err(error) => return Err(error.into()),
                };
                if bound
                    && readiness.readiness != "paused"
                    && self.store.account_model(id.clone()).await.is_ok()
                {
                    let mut engine = self.engine.lock().await;
                    self.store
                        .verify_account(
                            id.clone(),
                            ProviderIdentity {
                                id: observation.identity.id,
                                email: observation.identity.email,
                            },
                            now(),
                            false,
                        )
                        .await
                        .or_else(|error| {
                            if error.code() == "account-cooldown" {
                                Ok(())
                            } else {
                                Err(error)
                            }
                        })?;
                    if self
                        .store
                        .accounts()
                        .await?
                        .iter()
                        .any(|a| a.id == id && a.readiness == "ready")
                    {
                        self.store
                            .login_state(id.clone(), "ready".into(), None, None)
                            .await?;
                        engine.attach_account(id.clone(), bridge.clone());
                        engine.recover().await?;
                    }
                } else {
                    self.store
                        .login_state(
                            id.clone(),
                            "verifying".into(),
                            None,
                            session.login_expires_ms,
                        )
                        .await?;
                }
            }
            Err(error) if error.code == "authentication-expired" => {
                self.store.account_expired(id.clone(), now()).await?;
                let (state, expiry) = if ["ready", "expired"].contains(&session.lifecycle.as_str())
                {
                    ("expired", None)
                } else {
                    ("waiting-for-user", session.login_expires_ms)
                };
                self.store
                    .login_state(id.clone(), state.into(), Some(error.code), expiry)
                    .await?;
            }
            Err(error) if error.code == "account-rate-limited" => {
                self.store
                    .rate_limited(id.clone(), error.retry_at_ms, now())
                    .await?;
                self.store
                    .login_state(id.clone(), "ready".into(), Some(error.code), None)
                    .await?;
            }
            Err(error) => return Err(error.into()),
        }
        Ok(Duration::from_secs(5))
    }
}
