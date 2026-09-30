use super::*;

impl WorkerService {
    pub async fn tabs(&self) -> WorkerTabs {
        WorkerTabs {
            tabs: self
                .inner
                .state
                .read()
                .await
                .tabs
                .values()
                .cloned()
                .collect(),
        }
    }

    pub async fn open_tab(&self, request: TabOpen) -> Result<WorkerTab> {
        let account_id = self.resolve_account(request.account_id.as_deref()).await?;
        let tab_id = request
            .tab_id
            .unwrap_or_else(|| format!("tab-{}", uuid::Uuid::new_v4().simple()));
        validate_id(&tab_id, "tab")?;
        let model = normalize_model(&request.model)?;
        let workspace = self.inner.root.join("tabs").join(&tab_id);
        {
            let state = self.inner.state.read().await;
            if state.tabs.contains_key(&tab_id) {
                return Err(action(
                    "tab-exists",
                    "A tab with that ID already exists.",
                    "List tabs and choose a new tab_id.",
                ));
            }
        }
        std::fs::create_dir(&workspace).map_err(Error::Io)?;
        restrict_directory(&workspace)?;
        if let Some(object_id) = &request.input_object_id {
            let object = self.object(object_id).await?;
            let path = self.object_path(&object.object_id);
            if let Err(error) = extract_archive(&path, &workspace) {
                let _ = std::fs::remove_dir_all(&workspace);
                return Err(error);
            }
        }
        let tab = WorkerTab {
            tab_id: tab_id.clone(),
            account_id: account_id.clone(),
            status: "open".into(),
            model,
            reasoning_effort: request.reasoning_effort,
            active_job_id: None,
            input_object_id: request.input_object_id,
            created_ms: now_ms(),
        };
        let mut state = self.inner.state.write().await;
        state.tabs.insert(tab_id, tab.clone());
        if let Some(account) = state.accounts.get_mut(&account_id) {
            account.active_tabs += 1;
        }
        Ok(tab)
    }

    pub async fn close_tab(&self, request: TabRef) -> Result<WorkerTab> {
        let active = {
            let state = self.inner.state.read().await;
            let tab = state
                .tabs
                .get(&request.tab_id)
                .ok_or_else(|| not_found("tab"))?;
            tab.active_job_id.clone()
        };
        if let Some(job_id) = active {
            if !request.force {
                return Err(action(
                    "tab-busy",
                    "The tab has an active job.",
                    "Cancel the job or close the tab with force=true.",
                ));
            }
            let _ = self.cancel_job(JobRef { job_id }).await?;
        }
        let mut state = self.inner.state.write().await;
        let tab = state
            .tabs
            .get_mut(&request.tab_id)
            .ok_or_else(|| not_found("tab"))?;
        tab.status = "closed".into();
        let account_id = tab.account_id.clone();
        let tab = tab.clone();
        if let Some(account) = state.accounts.get_mut(&account_id) {
            account.active_tabs = account.active_tabs.saturating_sub(1);
        }
        Ok(tab)
    }

    pub async fn set_model(&self, request: TabModel) -> Result<WorkerTab> {
        let model = normalize_model(&request.model)?;
        let mut state = self.inner.state.write().await;
        let tab = state
            .tabs
            .get_mut(&request.tab_id)
            .ok_or_else(|| not_found("tab"))?;
        if tab.status != "open" || tab.active_job_id.is_some() {
            return Err(action(
                "tab-busy",
                "Model settings can change only on an idle open tab.",
                "Wait for or cancel the active job, then retry.",
            ));
        }
        tab.model = model;
        tab.reasoning_effort = request.reasoning_effort;
        Ok(tab.clone())
    }
}
