use super::helpers::*;
use super::*;

impl GitSparkApp {
    pub(crate) fn process_events(&mut self, cx: &mut Context<Self>) {
        self.event_tx.pending.store(false, Ordering::Release);
        let mut had_events = false;
        while let Ok(event) = self.event_rx.try_recv() {
            had_events = true;
            match event {
                AppEvent::RepoLoaded(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = "Repository loaded.".to_string();
                    self.messages.error_message.clear();
                }
                AppEvent::RepoLoaded(Err(err)) => {
                    self.messages.error_message = format!("Failed to open repository: {err}");
                }
                AppEvent::RepoRefreshed(path, Ok(snapshot), reason) => {
                    let should_apply = self
                        .repo_path()
                        .map(PathBuf::from)
                        .map(|current_path| current_path == path)
                        .unwrap_or(false);
                    if !should_apply {
                        continue;
                    }
                    self.adopt_snapshot(snapshot);
                    if reason == RepoRefreshReason::Manual {
                        self.messages.status_message = "Repository refreshed.".to_string();
                    }
                    self.messages.error_message.clear();
                }
                AppEvent::RepoRefreshed(path, Err(err), reason) => {
                    let should_apply = self
                        .repo_path()
                        .map(PathBuf::from)
                        .map(|current_path| current_path == path)
                        .unwrap_or(false);
                    if !should_apply {
                        continue;
                    }
                    if reason == RepoRefreshReason::Manual {
                        self.messages.error_message = format!("Refresh failed: {err}");
                    } else {
                        self.messages.error_message = err;
                    }
                }
                AppEvent::BranchSwitched(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = format!("Switched to branch '{branch}'.");
                    self.messages.error_message.clear();
                }
                AppEvent::BranchSwitched(Err(err), branch) => {
                    if branch_switch_needs_stash(&err) {
                        self.repo.switch_branch_bring_changes = false;
                        self.nav.active_dialog = ActiveDialog::StashAndSwitch {
                            target_branch: branch.clone(),
                        };
                        self.messages.error_message =
                            "Branch switch needs a clean working tree.".to_string();
                    } else {
                        self.messages.error_message = format!("Branch switch failed: {err}");
                    }
                }
                AppEvent::BranchMerged(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = format!("Merged '{branch}'.");
                    self.messages.error_message.clear();
                }
                AppEvent::BranchMerged(Err(err), branch) => {
                    self.refresh_git_operation_state(Some(branch));
                    if self.repo.operation.is_some() {
                        self.messages.status_message =
                            "Merge stopped because conflicts need attention.".to_string();
                    }
                    self.messages.error_message = format!("Merge failed: {err}");
                }
                AppEvent::CommitCreated(Ok(snapshot), summary) => {
                    self.adopt_snapshot(snapshot);
                    self.selection.selected_diff_lines.clear();
                    self.commit.summary.clear();
                    self.commit.body.clear();
                    self.summary_cursor = 0;
                    self.description_cursor = 0;
                    self.commit.ai_preview = None;
                    self.messages.status_message = "Commit created.".to_string();
                    self.messages.error_message.clear();
                    // Undo commit banner
                    self.nav.undo_commit = Some((summary, std::time::Instant::now()));
                }
                AppEvent::CommitCreated(Err(err), _) => {
                    self.messages.error_message = format!("Commit failed: {err}");
                }
                AppEvent::CommitUndone(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.nav.undo_commit = None;
                    self.messages.status_message = "Undid last commit.".to_string();
                    self.messages.error_message.clear();
                }
                AppEvent::CommitUndone(Err(err)) => {
                    self.messages.error_message = format!("Undo commit failed: {err}");
                }
                AppEvent::NetworkActionCompleted(Ok(snapshot), action_label) => {
                    self.network.active_action = None;
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = format!("{action_label} complete.");
                    self.messages.error_message.clear();
                }
                AppEvent::NetworkActionCompleted(Err(err), action_label) => {
                    self.network.active_action = None;
                    self.messages.error_message = format!("{action_label} failed: {err}");
                }
                AppEvent::AiCommitGenerated(Ok(suggestion)) => {
                    self.commit.ai_in_flight = false;
                    self.commit.summary = suggestion.subject.clone();
                    self.commit.body = suggestion.body.clone();
                    self.summary_cursor = self.commit.summary.len();
                    self.description_cursor = self.commit.body.len();
                    self.commit.ai_preview = Some(suggestion);
                    self.messages.status_message = "Generated commit suggestion.".to_string();
                    self.messages.error_message.clear();
                }
                AppEvent::AiCommitGenerated(Err(err)) => {
                    self.commit.ai_in_flight = false;
                    self.messages.error_message = format!("AI generation failed: {err}");
                }
                AppEvent::OpenRouterModelsLoaded(Ok(models)) => {
                    if self.settings.ai.provider == AiProvider::OpenRouter
                        && self.settings.ai.model.trim().is_empty()
                    {
                        if let Some(first) = models.first() {
                            self.settings.ai.model = first.id.clone();
                        }
                    }
                    self.filters.openrouter_models = OpenRouterModelsState::Ready(models);
                }
                AppEvent::OpenRouterModelsLoaded(Err(err)) => {
                    self.filters.openrouter_models = OpenRouterModelsState::Error(err);
                }
                AppEvent::CommitDiffLoaded(oid, Ok(diffs)) => {
                    if self.selection.selected_commit.as_deref() == Some(oid.as_str()) {
                        if let Some(first) = diffs.first() {
                            self.selection.selected_commit_file = Some(first.path.clone());
                        }
                        self.selection.commit_diffs = Some(diffs);
                    }
                }
                AppEvent::CommitDiffLoaded(_, Err(err)) => {
                    self.messages.error_message = format!("Failed to load commit details: {err}");
                }
                AppEvent::FileDiffRefreshed(path, Ok(entry)) => {
                    // Update the diff for this file in the current snapshot
                    if let Some(snapshot) = &mut self.repo.snapshot {
                        if let Some(existing) = snapshot.diffs.iter_mut().find(|d| d.path == path) {
                            *existing = entry;
                        }
                    }
                }
                AppEvent::FileDiffRefreshed(_, Err(_)) => {}
                AppEvent::RepoOperationCompleted(Ok(snapshot), _action_label, success_message) => {
                    self.add_recent_repo(snapshot.repo.path.clone());
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = success_message;
                    self.messages.error_message.clear();
                }
                AppEvent::RepoOperationCompleted(Err(err), action_label, _success_message) => {
                    let target_hint = self.repo.merge_target.trim().to_string();
                    self.refresh_git_operation_state(
                        (!target_hint.is_empty()).then_some(target_hint),
                    );
                    if self.repo.operation.is_some() {
                        self.messages.status_message =
                            format!("{action_label} stopped because conflicts need attention.");
                    }
                    self.messages.error_message = format!("{action_label} failed: {err}");
                }
                AppEvent::GitOperationControlCompleted(Ok(snapshot), action_label) => {
                    self.add_recent_repo(snapshot.repo.path.clone());
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = format!("{action_label} complete.");
                    self.messages.error_message.clear();
                }
                AppEvent::GitOperationControlCompleted(Err(err), action_label) => {
                    let target_hint = self
                        .repo
                        .operation
                        .as_ref()
                        .and_then(|operation| operation.target_branch.clone());
                    self.refresh_git_operation_state(target_hint);
                    self.messages.error_message = format!("{action_label} failed: {err}");
                }
                AppEvent::CommitDiffCopied(oid, Ok(diff_text)) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(diff_text));
                    self.messages.status_message =
                        format!("Copied diff for {}.", short_commit_label(&oid));
                    self.messages.error_message.clear();
                }
                AppEvent::CommitDiffCopied(oid, Err(err)) => {
                    self.messages.error_message = format!(
                        "Failed to copy diff for {}: {err}",
                        short_commit_label(&oid)
                    );
                }
                AppEvent::Automation(request) => {
                    let response = self.handle_automation_command(request.command, cx);
                    let _ = request.respond_to.send(response);
                }
            }
        }
        // Only trigger a re-render if we actually processed events.
        if had_events {
            cx.notify();
        }
    }

    // ------------------------------------------------------------------
    // Toolbar action handler
    // ------------------------------------------------------------------

    pub fn handle_toolbar_action(&mut self, action: ToolbarAction, cx: &mut Context<Self>) {
        self.close_history_context_menu();
        match action {
            ToolbarAction::ToggleRepoSelector => {
                self.nav.show_repo_selector = !self.nav.show_repo_selector;
                self.nav.show_branch_selector = false;
                self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                self.repo.pending_cherry_pick_oid = None;
                self.nav.show_network_dropdown = false;
            }
            ToolbarAction::SwitchBranch(name) => {
                self.repo.branch_target = name;
                self.switch_branch(cx);
            }
            ToolbarAction::RunNetworkAction(net_action) => {
                self.run_network_action(net_action, cx);
            }
            ToolbarAction::FetchOrigin => self.fetch_origin(cx),
            ToolbarAction::PullOrigin => self.pull_origin(cx),
            ToolbarAction::PushOrigin => self.push_origin(cx),
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Sidebar action handler
    // ------------------------------------------------------------------

    pub fn handle_sidebar_action(&mut self, action: SidebarAction, cx: &mut Context<Self>) {
        self.close_history_context_menu();
        match action {
            SidebarAction::OpenRepoDialog => self.open_repo_dialog(cx),
            SidebarAction::OpenRepo(path) => self.open_repo_with_notify(path, cx),
            SidebarAction::HideRepoSelector => self.nav.show_repo_selector = false,
            SidebarAction::SelectChange(path) => {
                if self.selection.selected_change.as_deref() != Some(path.as_str()) {
                    self.selection.selected_diff_lines.clear();
                }
                self.selection.selected_change = Some(path);
            }
            SidebarAction::DiscardChange(path) => self.discard_change(&path),
            SidebarAction::IgnorePath(path) => self.ignore_path(&path),
            SidebarAction::IgnoreExtension(ext) => self.ignore_extension(&ext),
            SidebarAction::CopyFullPath(path) => {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&path);
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        full_path.to_string_lossy().to_string(),
                    ));
                    self.messages.status_message = format!("Copied absolute path for '{path}'.");
                    self.messages.error_message.clear();
                }
            }
            SidebarAction::CopyRelativePath(path) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                self.messages.status_message = format!("Copied relative path for '{path}'.");
                self.messages.error_message.clear();
            }
            SidebarAction::RevealInFinder(path) => self.reveal_in_finder(&path),
            SidebarAction::OpenInEditor(path) => self.open_in_external_editor(&path),
            SidebarAction::OpenWithDefault(path) => self.open_with_default_program(&path),
            SidebarAction::SelectCommit(oid) => self.select_commit(oid, cx),
            SidebarAction::GenerateAiCommit => self.generate_ai_commit(cx),
            SidebarAction::ShowSettings => self.open_global_settings_modal(None, cx),
            SidebarAction::CommitAll => self.commit_all(cx),
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Settings action handler
    // ------------------------------------------------------------------

    pub fn handle_settings_action(&mut self, action: SettingsAction, cx: &mut Context<Self>) {
        match action {
            SettingsAction::SaveRemote => {
                self.save_remote_settings(cx);
                self.close_settings_modal_after_successful_save();
            }
            SettingsAction::SaveIgnoredFiles => {
                self.save_ignored_files_settings(cx);
                self.close_settings_modal_after_successful_save();
            }
            SettingsAction::SaveGitConfig => {
                self.save_git_config();
                self.close_settings_modal_after_successful_save();
            }
            SettingsAction::SaveAiSettings => {
                if self.settings.ai.provider == AiProvider::OpenRouter {
                    self.settings.ai.endpoint =
                        self.settings.ai.provider.default_endpoint().to_string();
                } else if self.settings.ai.endpoint.trim().is_empty() {
                    self.settings.ai.endpoint =
                        self.settings.ai.provider.default_endpoint().to_string();
                    self.settings_modal.ai_endpoint_cursor = self.settings.ai.endpoint.len();
                } else {
                    self.settings.ai.endpoint = self.settings.ai.endpoint.trim().to_string();
                    self.settings_modal.ai_endpoint_cursor = self.settings.ai.endpoint.len();
                }
                self.messages.error_message.clear();
                self.persist_settings();
                if self.messages.error_message.is_empty() {
                    self.messages.status_message = "AI settings saved.".to_string();
                    self.close_settings_modal_after_successful_save();
                }
            }
            SettingsAction::SetGitConfigScope(use_local) => {
                self.repo.use_local_identity = use_local;
                if use_local
                    && self.repo.local_identity.user_name.trim().is_empty()
                    && self.repo.local_identity.user_email.trim().is_empty()
                {
                    self.repo.local_identity.user_name = self.repo.identity.user_name.clone();
                    self.repo.local_identity.user_email = self.repo.identity.user_email.clone();
                }
                let (name_len, email_len) = {
                    let identity = self.active_git_settings_identity();
                    (identity.user_name.len(), identity.user_email.len())
                };
                self.settings_modal.git_user_name_cursor = name_len;
                self.settings_modal.git_user_email_cursor = email_len;
                self.settings_modal.git_user_name_selection = None;
                self.settings_modal.git_user_email_selection = None;
            }
            SettingsAction::ChangeProvider(provider) => {
                self.settings.ai.provider = provider;
                if self.settings.ai.provider == AiProvider::OpenRouter
                    || self.settings.ai.endpoint.trim().is_empty()
                {
                    self.settings.ai.endpoint =
                        self.settings.ai.provider.default_endpoint().to_string();
                    self.settings_modal.ai_endpoint_cursor = self.settings.ai.endpoint.len();
                    self.settings_modal.ai_endpoint_selection = None;
                }
                self.filters.openrouter_model_filter.clear();
                self.settings_modal.openrouter_model_filter_cursor = 0;
                if self.settings.ai.provider == AiProvider::OpenRouter {
                    self.settings_modal.active_field = Some(SettingsField::OpenRouterModelFilter);
                    self.ensure_openrouter_models(cx);
                } else {
                    self.settings_modal.active_field = Some(SettingsField::AiModel);
                    self.settings_modal.ai_model_cursor = self.settings.ai.model.len();
                }
            }
            SettingsAction::SelectOpenRouterModel(model_id) => {
                self.settings.ai.model = model_id;
                self.settings_modal.ai_model_cursor = self.settings.ai.model.len();
            }
            SettingsAction::RetryOpenRouterModels => {
                self.filters.openrouter_models = OpenRouterModelsState::Idle;
                self.ensure_openrouter_models(cx);
            }
            SettingsAction::Close => {
                self.close_settings_modal();
            }
        }
        cx.notify();
    }

    fn close_settings_modal_after_successful_save(&mut self) {
        if self.messages.error_message.is_empty() {
            self.close_settings_modal();
        }
    }

    // ------------------------------------------------------------------
    // Repository operations
    // ------------------------------------------------------------------

    pub(crate) fn open_repo_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = FileDialog::new().pick_folder() {
            self.open_repo_with_notify(path, cx);
        }
    }

    /// Open the Current Worktree picker, loading the list on first open.
    ///
    /// Deliberately synchronous: `git worktree list` reads a handful of
    /// administrative files and returns in single-digit milliseconds, so
    /// spawning a thread and re-rendering would cost more than it saves — and
    /// the list must be present in the frame the panel appears in, or the
    /// picker opens empty and fills in late.
    pub(crate) fn toggle_worktree_selector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let opening = !self.nav.show_worktree_selector;
        self.toggle_worktree_selector_headless(cx);
        if opening {
            window.focus(&self.worktree_filter_focus);
        }
    }

    /// The half that needs no `Window`, so the automation channel can drive it.
    pub(crate) fn toggle_worktree_selector_headless(&mut self, cx: &mut Context<Self>) {
        self.nav.show_worktree_selector = !self.nav.show_worktree_selector;
        self.nav.show_repo_selector = false;
        self.nav.show_branch_selector = false;
        self.nav.show_network_dropdown = false;

        if self.nav.show_worktree_selector {
            self.filters.worktree_filter_text.clear();
            self.worktree_filter_cursor = 0;
            if let Some(path) = self
                .repo
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.repo.path.clone())
            {
                match GitClient::new().list_worktrees(&path) {
                    Ok(worktrees) => self.repo.worktrees = worktrees,
                    Err(error) => {
                        self.messages.error_message = format!("Failed to list worktrees: {error}");
                        self.repo.worktrees.clear();
                    }
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn open_repo_with_notify(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_repo(path);
        cx.notify();
    }

    pub(crate) fn open_repo(&mut self, path: PathBuf) {
        self.messages.status_message = "Loading repository...".to_string();
        self.messages.error_message.clear();
        self.nav.show_repo_selector = false;
        self.stop_repo_watch();
        self.add_recent_repo(path.clone());
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.open_repo(path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoLoaded(res));
        });
    }

    fn default_repository_parent_path(&self) -> String {
        self.repo_path()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(dirs::document_dir)
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .to_string()
    }

    pub(crate) fn open_create_repository_dialog(&mut self, cx: &mut Context<Self>) {
        if self.repo.create_repo_path.trim().is_empty() {
            self.repo.create_repo_path = self.default_repository_parent_path();
        }
        if self.repo.create_repo_branch_name.trim().is_empty() {
            self.repo.create_repo_branch_name = "main".to_string();
        }
        self.repository_active_field = Some(RepositoryField::CreateName);
        self.repository_create_name_cursor = self.repo.create_repo_name.len();
        self.repository_create_name_selection = None;
        self.nav.active_dialog = ActiveDialog::CreateRepository;
        self.nav.show_repo_selector = false;
        self.nav.show_branch_selector = false;
        self.nav.show_network_dropdown = false;
        self.messages.error_message.clear();
        cx.notify();
    }

    pub(crate) fn open_clone_repository_dialog(&mut self, cx: &mut Context<Self>) {
        if self.repo.clone_repo_path.trim().is_empty() {
            self.repo.clone_repo_path = self.default_repository_parent_path();
            self.repository_clone_path_cursor = self.repo.clone_repo_path.len();
        }
        if self.repo.clone_repo_name.trim().is_empty() {
            self.repo.clone_repo_name = inferred_clone_directory_name(&self.repo.clone_repo_url);
            self.repository_clone_name_cursor = self.repo.clone_repo_name.len();
        }
        self.repository_active_field = Some(RepositoryField::CloneUrl);
        self.repository_clone_url_cursor = self.repo.clone_repo_url.len();
        self.repository_clone_url_selection = None;
        self.nav.active_dialog = ActiveDialog::CloneRepository;
        self.nav.show_repo_selector = false;
        self.nav.show_branch_selector = false;
        self.nav.show_network_dropdown = false;
        self.messages.error_message.clear();
        cx.notify();
    }

    pub(crate) fn create_repository_validation_message(&self) -> Option<String> {
        if self.repo.create_repo_name.trim().is_empty() {
            return Some("Type a repository name.".to_string());
        }
        if safe_repository_directory_name(&self.repo.create_repo_name).is_empty() {
            return Some(format!(
                "{} is not a valid repository name.",
                self.repo.create_repo_name.trim()
            ));
        }
        if self.repo.create_repo_path.trim().is_empty() {
            return Some("Choose a local path.".to_string());
        }
        if self.repo.create_repo_branch_name.trim().is_empty() {
            return Some("Type an initial branch name.".to_string());
        }
        let destination = PathBuf::from(self.repo.create_repo_path.trim())
            .join(safe_repository_directory_name(&self.repo.create_repo_name));
        if destination.exists() {
            if !destination.is_dir() {
                return Some(format!("{} is not a directory.", destination.display()));
            }
            if std::fs::read_dir(&destination)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(true)
            {
                return Some(format!(
                    "{} already exists and is not empty.",
                    destination.display()
                ));
            }
        }
        None
    }

    pub(crate) fn clone_repository_validation_message(&self) -> Option<String> {
        if self.repo.clone_repo_url.trim().is_empty() {
            return Some("Type a repository URL.".to_string());
        }
        if self.repo.clone_repo_path.trim().is_empty() {
            return Some("Choose a local path.".to_string());
        }
        let local_name = self.clone_repository_local_name();
        if local_name.is_empty() {
            return Some("Type a local repository name.".to_string());
        }
        let destination = PathBuf::from(self.repo.clone_repo_path.trim()).join(&local_name);
        if destination.exists() {
            if !destination.is_dir() {
                return Some(format!("{} is not a directory.", destination.display()));
            }
            if std::fs::read_dir(&destination)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(true)
            {
                return Some(format!(
                    "{} already exists and is not empty.",
                    destination.display()
                ));
            }
        }
        None
    }

    pub(crate) fn create_repository(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = self.create_repository_validation_message() {
            self.messages.error_message = message;
            cx.notify();
            return;
        }
        let parent_path = PathBuf::from(self.repo.create_repo_path.trim());
        let options = CreateRepositoryOptions {
            name: self.repo.create_repo_name.trim().to_string(),
            description: self.repo.create_repo_description.trim().to_string(),
            branch_name: self.repo.create_repo_branch_name.trim().to_string(),
            initialize_readme: self.repo.create_repo_initialize_readme,
            gitignore_template: self.repo.create_repo_gitignore_template.clone(),
            license_template: self.repo.create_repo_license_template.clone(),
            initial_commit: self.repo.create_repo_initial_commit,
        };
        let name = options.name.clone();

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = format!("Creating repository '{name}'...");
        self.messages.error_message.clear();
        self.stop_repo_watch();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .create_repository_with_options(&parent_path, options)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoOperationCompleted(
                res,
                "Create repository".to_string(),
                format!("Created repository '{name}'."),
            ));
        });
        cx.notify();
    }

    pub(crate) fn clone_repository(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = self.clone_repository_validation_message() {
            self.messages.error_message = message;
            cx.notify();
            return;
        }
        let url = self.repo.clone_repo_url.trim().to_string();
        let parent_path = PathBuf::from(self.repo.clone_repo_path.trim());
        let local_name = self.clone_repository_local_name();

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = format!("Cloning '{url}'...");
        self.messages.error_message.clear();
        self.stop_repo_watch();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .clone_repository_into(&url, &parent_path, &local_name)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoOperationCompleted(
                res,
                "Clone repository".to_string(),
                format!("Cloned repository from '{url}'."),
            ));
        });
        cx.notify();
    }

    pub(crate) fn clone_repository_local_name(&self) -> String {
        let explicit = safe_repository_directory_name(&self.repo.clone_repo_name);
        if explicit.is_empty() {
            inferred_clone_directory_name(&self.repo.clone_repo_url)
        } else {
            explicit
        }
    }

    pub fn refresh_repo(&mut self, cx: &mut Context<Self>) {
        self.request_repo_refresh(RepoRefreshReason::Manual, cx);
    }

    pub(crate) fn request_repo_refresh(
        &mut self,
        reason: RepoRefreshReason,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        if reason == RepoRefreshReason::Manual {
            self.messages.status_message = "Refreshing repository...".to_string();
        }
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let event_path = path.clone();
        thread::spawn(move || {
            let res = git.refresh_repo(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoRefreshed(event_path, res, reason));
        });
        cx.notify();
    }

    fn stop_repo_watch(&mut self) {
        self.repo_watch_generation.fetch_add(1, Ordering::SeqCst);
        self.watched_repo_path = None;
    }

    fn ensure_repo_watch(&mut self, repo_path: &Path) {
        if self.watched_repo_path.as_deref() == Some(repo_path) {
            return;
        }

        let path = repo_path.to_path_buf();
        let token = self.repo_watch_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.watched_repo_path = Some(path.clone());

        let generation = Arc::clone(&self.repo_watch_generation);
        let tx = self.event_tx.clone();

        thread::spawn(move || {
            let git = GitClient::new();
            let mut last_fingerprint = git.read_watch_fingerprint(&path).ok();

            while generation.load(Ordering::SeqCst) == token {
                thread::sleep(Duration::from_millis(3000));

                if generation.load(Ordering::SeqCst) != token {
                    break;
                }

                let Ok(current_fingerprint) = git.read_watch_fingerprint(&path) else {
                    continue;
                };

                let changed = match &last_fingerprint {
                    Some(previous) => previous != &current_fingerprint,
                    None => true,
                };

                if !changed {
                    continue;
                }

                last_fingerprint = Some(current_fingerprint);
                let res = git.refresh_repo(&path).map_err(|e| e.to_string());
                let _ = tx.send(AppEvent::RepoRefreshed(
                    path.clone(),
                    res,
                    RepoRefreshReason::Watch,
                ));
            }
        });
    }

    // ------------------------------------------------------------------
    // Network operations
    // ------------------------------------------------------------------

    pub(crate) fn fetch_origin(&mut self, cx: &mut Context<Self>) {
        self.run_network_action(NetworkAction::Fetch, cx);
    }

    pub(crate) fn pull_origin(&mut self, cx: &mut Context<Self>) {
        self.run_network_action(NetworkAction::Pull, cx);
    }

    pub(crate) fn push_origin(&mut self, cx: &mut Context<Self>) {
        if self
            .repo
            .snapshot
            .as_ref()
            .map(NetworkAction::from_snapshot)
            == Some(NetworkAction::PublishRepository)
        {
            self.run_network_action(NetworkAction::PublishRepository, cx);
            return;
        }

        self.run_network_action(NetworkAction::Push, cx);
    }

    pub(crate) fn run_network_action(&mut self, action: NetworkAction, cx: &mut Context<Self>) {
        if self.network.active_action.is_some() {
            return;
        }

        if action == NetworkAction::PublishRepository {
            self.prepare_publish_dialog();
            self.nav.active_dialog = ActiveDialog::PublishRepository;
            cx.notify();
            return;
        }

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let remote_name = self
            .repo
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.repo.remote_name.clone())
            .unwrap_or_else(|| "origin".to_string());
        let action_label = action.title(&remote_name);

        self.messages.status_message = format!("{action_label}...");
        self.messages.error_message.clear();
        self.network.active_action = Some(action);

        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let action_label_for_event = action_label.clone();

        thread::spawn(move || {
            let res = match action {
                NetworkAction::Fetch => git.fetch_origin(&path),
                NetworkAction::Pull => git.pull_origin(&path),
                NetworkAction::Push | NetworkAction::PublishBranch => git.push_origin(&path),
                NetworkAction::PublishRepository => unreachable!("handled before background run"),
            }
            .map_err(|e| e.to_string());

            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                action_label_for_event,
            ));
        });
        cx.notify();
    }

    pub(crate) fn open_publish_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prepare_publish_dialog();
        self.nav.active_dialog = ActiveDialog::PublishRepository;
        self.nav.show_network_dropdown = false;
        self.publish_active_field = Some(PublishField::Name);
        self.publish_name_cursor = self.network.publish_name.len();
        self.publish_name_selection = None;
        window.focus(&self.publish_focus);
        cx.notify();
    }

    fn prepare_publish_dialog(&mut self) {
        if self.network.publish_name.trim().is_empty() {
            if let Some(snapshot) = &self.repo.snapshot {
                self.network.publish_name = snapshot.repo.name.clone();
            }
        }
        self.publish_name_cursor = self
            .publish_name_cursor
            .min(self.network.publish_name.len());
        self.publish_description_cursor = self
            .publish_description_cursor
            .min(self.network.publish_description.len());
    }

    pub(crate) fn publish_repository(&mut self, cx: &mut Context<Self>) {
        if self.network.active_action.is_some() {
            return;
        }

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let name = self.network.publish_name.trim().to_string();
        if name.is_empty() {
            self.messages.error_message = "Repository name is required.".to_string();
            return;
        }

        let description = self.network.publish_description.trim().to_string();
        let private = self.network.publish_private;

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = "Publishing repository...".to_string();
        self.messages.error_message.clear();
        self.network.active_action = Some(NetworkAction::PublishRepository);

        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .publish_repository(&path, &name, &description, private)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                "Publish repository".to_string(),
            ));
        });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Branch operations
    // ------------------------------------------------------------------

    fn switch_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let target = self.repo.branch_target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch first.".to_string();
            return;
        }

        if self.repo.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.repo.current_branch != target && !snapshot.changes.is_empty()
        }) {
            self.repo.switch_branch_bring_changes = false;
            self.nav.active_dialog = ActiveDialog::StashAndSwitch {
                target_branch: target,
            };
            self.messages.error_message = "Branch switch needs a clean working tree.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Switching to '{}'...", target);
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.switch_branch(&path, &target).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchSwitched(res, target));
        });
        cx.notify();
    }

    pub(crate) fn stash_and_switch_branch(&mut self, target: String, cx: &mut Context<Self>) {
        self.nav.active_dialog = ActiveDialog::None;
        self.repo.switch_branch_bring_changes = false;

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let target = target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch first.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Stashing changes and switching to '{target}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .stash_all(&path)
                .and_then(|_| git.switch_branch(&path, &target))
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchSwitched(res, target));
        });
        cx.notify();
    }

    pub(crate) fn switch_branch_with_changes(&mut self, target: String, cx: &mut Context<Self>) {
        self.nav.active_dialog = ActiveDialog::None;
        self.repo.switch_branch_bring_changes = false;

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let target = target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch first.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Switching to '{target}' with changes...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.switch_branch(&path, &target).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchSwitched(res, target));
        });
        cx.notify();
    }

    pub(crate) fn show_stash_changes_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        if snapshot.changes.is_empty() {
            self.messages.error_message = "There are no local changes to stash.".to_string();
            cx.notify();
            return;
        }

        self.nav.active_dialog = ActiveDialog::StashChanges;
        self.messages.error_message.clear();
        cx.notify();
    }

    pub(crate) fn stash_changes(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };
        if self
            .repo
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.changes.is_empty())
            .unwrap_or(true)
        {
            self.messages.error_message = "There are no local changes to stash.".to_string();
            cx.notify();
            return;
        }

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = "Stashing changes...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.stash_all(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                "Stashed changes".to_string(),
            ));
        });
        cx.notify();
    }

    pub(crate) fn restore_stash(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };
        if self.repo.stash_files.is_empty() {
            self.messages.error_message =
                "Load the stashed file list before restoring the stash.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = "Restoring stash...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.stash_pop(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                "Restored stash".to_string(),
            ));
        });
        cx.notify();
    }

    pub(crate) fn show_discard_stash_dialog(&mut self, cx: &mut Context<Self>) {
        if self.repo.stash_files.is_empty() {
            if let Some(path) = self.repo_path().map(PathBuf::from) {
                match self.git.latest_stash_files(&path) {
                    Ok(files) => {
                        self.repo.stash_files = files;
                        self.messages.error_message.clear();
                    }
                    Err(err) => {
                        self.repo.stash_files.clear();
                        self.messages.error_message = format!("Could not read stash files: {err}");
                    }
                }
            }
        }
        self.nav.active_dialog = ActiveDialog::DiscardStash;
        cx.notify();
    }

    pub(crate) fn discard_stash(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };
        if self.repo.stash_files.is_empty() {
            self.messages.error_message =
                "Load the stashed file list before discarding the stash.".to_string();
            cx.notify();
            return;
        }

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = "Discarding stash...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.stash_drop(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                "Discarded stash".to_string(),
            ));
        });
        cx.notify();
    }

    pub(crate) fn show_restore_stash_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.repo_path().map(PathBuf::from) {
            match self.git.latest_stash_files(&path) {
                Ok(files) => {
                    self.repo.stash_files = files;
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.repo.stash_files.clear();
                    self.messages.error_message = format!("Could not read stash files: {err}");
                }
            }
        } else {
            self.repo.stash_files.clear();
        }
        self.nav.active_dialog = ActiveDialog::RestoreStash;
        cx.notify();
    }

    pub(crate) fn create_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        // Use filter text as branch name if new_branch_name is empty
        let proposed_name = if self.repo.new_branch_name.trim().is_empty() {
            self.filters.branch_filter_text.trim()
        } else {
            self.repo.new_branch_name.trim()
        };
        let name = sanitized_ref_name(proposed_name);
        if name.is_empty() {
            self.messages.error_message = if proposed_name.is_empty() {
                "Type a branch name in the filter field, then click New Branch.".to_string()
            } else {
                format!("{proposed_name} is not a valid name.")
            };
            cx.notify();
            return;
        }
        if self.branch_name_exists(&name) {
            self.messages.error_message = format!("A branch named {name} already exists.");
            cx.notify();
            return;
        }

        let start_point = self.repo.new_branch_start_point.clone();
        self.messages.status_message = format!("Creating branch '{name}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = match start_point {
                Some(oid) => git.create_branch_from_commit(&path, &name, &oid),
                None => git.create_branch(&path, &name),
            }
            .map_err(|e| e.to_string());
            tx.send(AppEvent::BranchSwitched(res, name));
        });
        self.repo.new_branch_name.clear();
        self.repo.new_branch_start_point = None;
        self.new_branch_cursor = 0;
        self.new_branch_selection = None;
        self.filters.branch_filter_text.clear();
        self.branch_filter_cursor = 0;
        self.nav.show_branch_selector = false;
        self.nav.branch_selector_mode = BranchSelectorMode::Switch;
        self.nav.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub(crate) fn rename_branch(&mut self, old_name: String, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let proposed_name = self.repo.new_branch_name.trim();
        let new_name = sanitized_ref_name(proposed_name);
        if new_name.is_empty() {
            self.messages.error_message = if proposed_name.is_empty() {
                "Type a new branch name.".to_string()
            } else {
                format!("{proposed_name} is not a valid name.")
            };
            cx.notify();
            return;
        }
        if new_name == old_name {
            self.nav.active_dialog = ActiveDialog::None;
            cx.notify();
            return;
        }
        if self.branch_name_exists(&new_name) {
            self.messages.error_message = format!("A branch named {new_name} already exists.");
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Renaming branch '{old_name}' to '{new_name}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let old = old_name.clone();
        let new_name_for_event = new_name.clone();
        thread::spawn(move || {
            let res = git
                .rename_branch(&path, &old, &new_name)
                .map_err(|e| e.to_string());
            tx.send(AppEvent::NetworkActionCompleted(
                res,
                format!("Renamed branch to '{new_name_for_event}'"),
            ));
        });
        self.repo.new_branch_name.clear();
        self.new_branch_cursor = 0;
        self.new_branch_selection = None;
        self.nav.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub(crate) fn create_tag(&mut self, target_oid: String, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let tag_name = self.repo.new_branch_name.trim().to_string();
        if let Some(message) = self.create_tag_validation_message() {
            self.messages.error_message = message;
            cx.notify();
            return;
        }

        self.messages.status_message = format!(
            "Creating tag '{tag_name}' on {}...",
            short_commit_label(&target_oid)
        );
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let tag_name_for_event = tag_name.clone();
        thread::spawn(move || {
            let res = git
                .create_tag(&path, &target_oid, &tag_name)
                .map_err(|e| e.to_string());
            tx.send(AppEvent::NetworkActionCompleted(
                res,
                format!("Created tag '{tag_name_for_event}'"),
            ));
        });
        self.repo.new_branch_name.clear();
        self.new_branch_cursor = 0;
        self.new_branch_selection = None;
        self.nav.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub(crate) fn delete_tag(&mut self, tag_name: String, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let tag_name = tag_name.trim().to_string();
        if tag_name.is_empty() {
            self.messages.error_message = "Tag name cannot be empty.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Deleting tag '{tag_name}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let tag_name_for_event = tag_name.clone();
        thread::spawn(move || {
            let git = GitClient::new();
            let res = git.delete_tag(&path, &tag_name).map_err(|e| e.to_string());
            tx.send(AppEvent::NetworkActionCompleted(
                res,
                format!("Deleted tag '{tag_name_for_event}'"),
            ));
        });
        self.nav.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub fn merge_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let target = self.repo.merge_target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch to merge.".to_string();
            return;
        }

        self.messages.status_message = format!("Merging '{}'...", target);
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.merge_branch(&path, &target).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchMerged(res, target));
        });
        cx.notify();
    }

    pub fn update_from_default_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let current_branch = snapshot.repo.current_branch.clone();
        let default_branch = self.default_branch_name();
        if current_branch == default_branch {
            self.messages.error_message = format!("Current branch is already '{default_branch}'.");
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Updating from '{default_branch}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .update_current_branch_from(&path, &default_branch)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoOperationCompleted(
                res,
                "Update from default branch".to_string(),
                format!("Updated '{current_branch}' from '{default_branch}'."),
            ));
        });
        cx.notify();
    }

    pub fn rebase_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let current_branch = snapshot.repo.current_branch.clone();
        let target = self.repo.merge_target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch to rebase onto.".to_string();
            cx.notify();
            return;
        }
        if current_branch == target {
            self.messages.error_message = "Choose another branch to rebase onto.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Rebasing '{current_branch}' onto '{target}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .rebase_current_branch_onto(&path, &target)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoOperationCompleted(
                res,
                "Rebase branch".to_string(),
                format!("Rebased '{current_branch}' onto '{target}'."),
            ));
        });
        cx.notify();
    }

    pub(crate) fn continue_git_operation(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };
        let Some(operation) = self.repo.operation.as_ref().cloned() else {
            self.messages.error_message = "No merge or rebase is in progress.".to_string();
            cx.notify();
            return;
        };

        let action_label = match operation.kind {
            GitOperationKind::Merge => "Continue merge",
            GitOperationKind::Rebase => "Continue rebase",
        }
        .to_string();
        self.messages.status_message = format!("{action_label}...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let git = GitClient::new();
            let res = match operation.kind {
                GitOperationKind::Merge => git.continue_merge(&path),
                GitOperationKind::Rebase => git.continue_rebase(&path),
            }
            .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::GitOperationControlCompleted(res, action_label));
        });
        cx.notify();
    }

    pub(crate) fn skip_rebase_operation(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };
        if self
            .repo
            .operation
            .as_ref()
            .is_none_or(|operation| operation.kind != GitOperationKind::Rebase)
        {
            self.messages.error_message = "No rebase is in progress.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = "Skipping rebase commit...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let git = GitClient::new();
            let res = git.skip_rebase(&path).map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::GitOperationControlCompleted(
                res,
                "Skip rebase".to_string(),
            ));
        });
        cx.notify();
    }

    pub(crate) fn abort_git_operation(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };
        let Some(operation) = self.repo.operation.as_ref().cloned() else {
            self.messages.error_message = "No merge or rebase is in progress.".to_string();
            cx.notify();
            return;
        };

        let action_label = match operation.kind {
            GitOperationKind::Merge => "Abort merge",
            GitOperationKind::Rebase => "Abort rebase",
        }
        .to_string();
        self.messages.status_message = format!("{action_label}...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let git = GitClient::new();
            let res = match operation.kind {
                GitOperationKind::Merge => git.abort_merge(&path),
                GitOperationKind::Rebase => git.abort_rebase(&path),
            }
            .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::GitOperationControlCompleted(res, action_label));
        });
        cx.notify();
    }

    pub(crate) fn open_conflict_in_editor(
        &mut self,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        self.open_in_external_editor(&relative_path);
        cx.notify();
    }

    pub(crate) fn reveal_conflict_file(&mut self, relative_path: String, cx: &mut Context<Self>) {
        self.reveal_in_finder(&relative_path);
        cx.notify();
    }

    pub(crate) fn mark_conflict_resolved(&mut self, relative_path: String, cx: &mut Context<Self>) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        match self.git.mark_conflict_resolved(&repo_path, &relative_path) {
            Ok(operation) => {
                self.repo.operation = operation;
                self.messages.status_message = format!("Marked '{}' resolved.", relative_path);
                self.messages.error_message.clear();
                self.request_repo_refresh(RepoRefreshReason::Manual, cx);
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Could not mark '{}' resolved: {err}", relative_path);
                cx.notify();
            }
        }
    }

    // ------------------------------------------------------------------
    // Commit operations
    // ------------------------------------------------------------------

    pub(crate) fn commit_all(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let summary = self.commit.summary.trim();
        if summary.is_empty() {
            self.messages.error_message = "Commit summary cannot be empty.".to_string();
            return;
        }
        let summary = summary.to_string();

        if let Some(message) = self.missing_identity_message() {
            self.messages.error_message = message.to_string();
            return;
        }

        let message = if self.commit.body.trim().is_empty() {
            summary.clone()
        } else {
            format!("{}\n\n{}", summary, self.commit.body.trim())
        };
        let summary_for_event = summary;
        let included_paths = self.included_commit_paths();
        let excluded_lines = self.selection.selected_diff_lines.clone();
        let partial_line_diff = if excluded_lines.is_empty() {
            None
        } else {
            let Some(diff) = self.selected_diff().cloned() else {
                self.messages.error_message = "No file diff selected.".to_string();
                return;
            };
            if diff.is_binary || diff.is_image || diff.is_submodule {
                self.messages.error_message =
                    "Line-level commits are only available for text diffs.".to_string();
                return;
            }
            Some(diff)
        };
        let included_lines = partial_line_diff
            .as_ref()
            .map(|diff| {
                crate::ui::workspace::selectable_diff_line_targets(&diff.path, &diff.diff, false)
                    .into_iter()
                    .filter(|target| !excluded_lines.contains(target))
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();

        self.messages.status_message = "Creating commit...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = if let Some(diff) = partial_line_diff {
                git.head_file_text(&path, &diff.path)
                    .map_err(|e| e.to_string())
                    .and_then(|base_text| {
                        crate::ui::diff_line_discard::apply_selected_lines_to_base_text(
                            &diff.path,
                            &diff.diff,
                            &base_text,
                            &included_lines,
                        )
                    })
                    .and_then(|selected_content| {
                        git.commit_paths_with_path_content(
                            &path,
                            included_paths.as_deref(),
                            &diff.path,
                            &selected_content,
                            &message,
                        )
                        .map_err(|e| e.to_string())
                    })
            } else if let Some(paths) = included_paths {
                git.commit_paths(&path, &paths, &message)
                    .map_err(|e| e.to_string())
            } else {
                git.commit_all(&path, &message).map_err(|e| e.to_string())
            };
            let _ = tx.send(AppEvent::CommitCreated(res, summary_for_event));
        });
        cx.notify();
    }

    pub(crate) fn undo_last_commit(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            return;
        };
        if !self.can_undo_last_commit() {
            self.messages.error_message =
                "Cannot undo the last commit while it has tags.".to_string();
            cx.notify();
            return;
        }
        self.nav.undo_commit = None;
        self.messages.status_message = "Undoing last commit...".to_string();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.undo_last_commit(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitUndone(res));
        });
        cx.notify();
    }

    pub(crate) fn toggle_diff_line_selection(
        &mut self,
        target: DiffLineSelection,
        cx: &mut Context<Self>,
    ) {
        if self.nav.diff_options.hide_whitespace_changes {
            return;
        }

        if self.selection.selected_change.as_deref() != Some(target.path.as_str()) {
            self.selection.selected_diff_lines.clear();
        }

        if !self.selection.selected_diff_lines.insert(target.clone()) {
            self.selection.selected_diff_lines.remove(&target);
        }

        cx.notify();
    }

    // ------------------------------------------------------------------
    // AI commit generation
    // ------------------------------------------------------------------

    pub(crate) fn generate_ai_commit(&mut self, cx: &mut Context<Self>) {
        if self.commit.ai_in_flight {
            return;
        }

        let Some(snapshot) = &self.repo.snapshot else {
            self.messages.error_message =
                "Open a repository before generating a commit message.".to_string();
            return;
        };

        let diff = snapshot
            .diffs
            .iter()
            .filter(|entry| !entry.is_binary)
            .map(|entry| format!("FILE: {}\n{}", entry.path, entry.diff))
            .collect::<Vec<_>>()
            .join("\n\n");

        if diff.trim().is_empty() {
            self.messages.error_message =
                "No text diff available for AI commit generation.".to_string();
            return;
        }

        self.messages.status_message = "Generating AI commit suggestion...".to_string();
        self.messages.error_message.clear();
        self.commit.ai_in_flight = true;
        let tx = self.event_tx.clone();
        let ai = AiClient::new();
        let settings = self.settings.ai.clone();
        thread::spawn(move || {
            let res = ai
                .generate_commit_message(&settings, &diff)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::AiCommitGenerated(res));
        });
        cx.notify();
    }

    pub(crate) fn ensure_openrouter_models(&mut self, cx: &mut Context<Self>) {
        if self.settings.ai.provider != AiProvider::OpenRouter {
            return;
        }

        match self.filters.openrouter_models {
            OpenRouterModelsState::Idle | OpenRouterModelsState::Error(_) => {}
            OpenRouterModelsState::Loading | OpenRouterModelsState::Ready(_) => return,
        }

        self.filters.openrouter_models = OpenRouterModelsState::Loading;
        let tx = self.event_tx.clone();
        let ai = AiClient::new();
        thread::spawn(move || {
            let res = ai.fetch_openrouter_models().map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::OpenRouterModelsLoaded(res));
        });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Git config
    // ------------------------------------------------------------------

    fn save_git_config(&mut self) {
        if self.settings_has_repository_scope()
            && let Some(path) = self.repo_path().map(PathBuf::from)
        {
            let write_result = if self.repo.use_local_identity {
                if !git_author_name_is_valid(&self.repo.local_identity.user_name) {
                    self.messages.error_message = INVALID_GIT_AUTHOR_NAME_MESSAGE.to_string();
                    self.messages.status_message.clear();
                    return;
                }
                self.git.write_identity(&path, &self.repo.local_identity)
            } else {
                self.git.clear_local_author_identity(&path).and_then(|_| {
                    self.git.write_global_default_branch(
                        self.repo.global_identity.default_branch.as_deref(),
                    )
                })
            };

            match write_result {
                Ok(()) => {
                    if let Err(err) = self
                        .git
                        .write_pull_rebase(&path, self.repo.identity.pull_rebase)
                    {
                        self.messages.error_message = format!(
                            "Saved repository Git identity, but failed to save repository pull behavior: {err}"
                        );
                        return;
                    }

                    self.settings.default_branch =
                        self.active_git_settings_identity().default_branch.clone();
                    self.persist_settings();
                    self.load_identity(&path);
                    self.messages.status_message = "Git config saved.".to_string();
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message =
                        format!("Failed to save repository Git config: {err}");
                }
            }
        } else {
            if !git_author_name_is_valid(&self.repo.global_identity.user_name) {
                self.messages.error_message = INVALID_GIT_AUTHOR_NAME_MESSAGE.to_string();
                self.messages.status_message.clear();
                return;
            }
            match self.git.write_global_identity(&self.repo.global_identity) {
                Ok(()) => {
                    if let Some(path) = self.repo_path().map(PathBuf::from) {
                        self.load_identity(&path);
                    } else {
                        self.load_global_identity();
                    }
                    self.settings.default_branch = self.repo.global_identity.default_branch.clone();
                    self.persist_settings();
                    self.messages.status_message = "Git config saved.".to_string();
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message =
                        format!("Failed to save global Git config: {err}");
                }
            }
        }
    }

    fn save_remote_settings(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            self.messages.status_message.clear();
            return;
        };
        let Some(remote_name) = self.repo.remote_name.clone() else {
            self.messages.error_message = "This repository does not have a remote.".to_string();
            self.messages.status_message.clear();
            return;
        };
        let remote_url = self.repo.remote_url.trim().to_string();
        if remote_url.is_empty() {
            self.messages.error_message = "Remote URL cannot be empty.".to_string();
            self.messages.status_message.clear();
            return;
        }

        match self.git.set_remote_url(&path, &remote_name, &remote_url) {
            Ok(snapshot) => {
                self.add_recent_repo(snapshot.repo.path.clone());
                self.adopt_snapshot(snapshot);
                self.messages.status_message = "Remote settings saved.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!("Failed to save remote settings: {err}");
                self.messages.status_message.clear();
            }
        }
        cx.notify();
    }

    fn save_ignored_files_settings(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            self.messages.status_message.clear();
            return;
        };

        match self
            .git
            .write_gitignore(&path, &self.repo.ignored_files_text)
        {
            Ok(snapshot) => {
                self.add_recent_repo(snapshot.repo.path.clone());
                self.adopt_snapshot(snapshot);
                self.messages.status_message = "Ignored files saved.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!("Failed to save ignored files: {err}");
                self.messages.status_message.clear();
            }
        }
        cx.notify();
    }

    pub(crate) fn load_identity(&mut self, path: &Path) {
        self.load_global_identity();
        match self.git.read_identity(path) {
            Ok(mut identity) => {
                // Fall back to app settings default branch if git config doesn't have one
                if identity.default_branch.is_none() {
                    identity.default_branch = self.settings.default_branch.clone();
                }
                self.repo.identity = identity;
            }
            Err(err) => {
                self.repo.identity = GitIdentity::default();
                self.messages.error_message = format!("Could not load git config: {err}");
            }
        }

        match self.git.read_local_identity(path) {
            Ok(mut identity) => {
                if identity.default_branch.is_none() {
                    identity.default_branch = self.settings.default_branch.clone();
                }
                self.repo.use_local_identity =
                    !identity.user_name.trim().is_empty() || !identity.user_email.trim().is_empty();
                self.repo.local_identity = identity;
            }
            Err(err) => {
                self.repo.use_local_identity = false;
                self.repo.local_identity = GitIdentity::default();
                self.messages.error_message = format!("Could not load local Git config: {err}");
            }
        }
    }

    pub(crate) fn load_global_identity(&mut self) {
        match self.git.read_global_identity() {
            Ok(mut identity) => {
                if identity.default_branch.is_none() {
                    identity.default_branch = self.settings.default_branch.clone();
                }
                self.repo.global_identity = identity;
            }
            Err(err) => {
                self.repo.global_identity = GitIdentity::default();
                self.messages.error_message = format!("Could not load global Git config: {err}");
            }
        }
    }

    pub(crate) fn load_remote_settings(&mut self, path: &Path) {
        match self.git.primary_remote(path) {
            Ok(Some((name, url))) => {
                self.repo.remote_name = Some(name);
                self.repo.remote_url = url;
                self.settings_modal.remote_url_cursor = self.repo.remote_url.len();
                self.settings_modal.remote_url_selection = None;
            }
            Ok(None) => {
                self.repo.remote_name = None;
                self.repo.remote_url.clear();
                self.settings_modal.remote_url_cursor = 0;
                self.settings_modal.remote_url_selection = None;
            }
            Err(err) => {
                self.repo.remote_name = None;
                self.repo.remote_url.clear();
                self.settings_modal.remote_url_cursor = 0;
                self.settings_modal.remote_url_selection = None;
                self.messages.error_message = format!("Could not load remote settings: {err}");
            }
        }
    }

    pub(crate) fn load_ignored_files_settings(&mut self, path: &Path) {
        match self.git.read_gitignore(path) {
            Ok(text) => {
                self.repo.ignored_files_text = text;
                self.settings_modal.ignored_files_cursor = self.repo.ignored_files_text.len();
                self.settings_modal.ignored_files_selection = None;
            }
            Err(err) => {
                self.repo.ignored_files_text.clear();
                self.settings_modal.ignored_files_cursor = 0;
                self.settings_modal.ignored_files_selection = None;
                self.messages.error_message = format!("Could not load ignored files: {err}");
            }
        }
    }

    // ------------------------------------------------------------------
    // Commit diff / selection
    // ------------------------------------------------------------------

    fn load_commit_diff(&mut self, oid: String) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            return;
        };

        let tx = self.event_tx.clone();
        let git = GitClient::new();

        thread::spawn(move || {
            let res = git.get_commit_diff(&path, &oid).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitDiffLoaded(oid, res));
        });
    }

    /// Expand diff context in-memory for a specific hunk.
    pub fn expand_diff_context(
        &mut self,
        file_path: String,
        hunk_index: usize,
        direction: DiffExpandDirection,
    ) {
        let Some(snapshot) = &mut self.repo.snapshot else {
            return;
        };
        let Some(entry) = snapshot.diffs.iter_mut().find(|d| d.path == file_path) else {
            return;
        };
        let Some(file_lines) = &entry.file_contents else {
            return;
        };

        // Save original diff for collapse (only on first expansion)
        if entry.original_diff.is_none() {
            entry.original_diff = Some(entry.diff.clone());
        }

        let new_diff = crate::ui::workspace::expand_diff_in_memory(
            &entry.diff,
            file_lines,
            hunk_index,
            direction,
        );
        entry.diff = new_diff;
    }

    /// Collapse expanded diff back to original.
    pub fn collapse_diff(&mut self, file_path: String) {
        let Some(snapshot) = &mut self.repo.snapshot else {
            return;
        };
        let Some(entry) = snapshot.diffs.iter_mut().find(|d| d.path == file_path) else {
            return;
        };
        if let Some(original) = entry.original_diff.take() {
            entry.diff = original;
        }
    }

    pub fn toggle_hide_whitespace_changes(&mut self, cx: &mut Context<Self>) {
        self.nav.diff_options.hide_whitespace_changes =
            !self.nav.diff_options.hide_whitespace_changes;
        if self.nav.diff_options.hide_whitespace_changes {
            self.selection.selected_diff_lines.clear();
        }
        cx.notify();
    }

    pub fn toggle_side_by_side_diff(&mut self, cx: &mut Context<Self>) {
        self.nav.diff_options.show_side_by_side = !self.nav.diff_options.show_side_by_side;
        cx.notify();
    }

    pub(crate) fn discard_selected_diff_lines(&mut self, cx: &mut Context<Self>) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };
        let Some(diff) = self.selected_diff().cloned() else {
            self.messages.error_message = "No file diff selected.".to_string();
            cx.notify();
            return;
        };
        if diff.is_binary || diff.is_image || diff.is_submodule {
            self.messages.error_message =
                "Selected line discard is only available for text diffs.".to_string();
            cx.notify();
            return;
        }

        let selected_lines = self.selection.selected_diff_lines.clone();
        let file_path = repo_path.join(&diff.path);
        let file_text = match std::fs::read_to_string(&file_path) {
            Ok(text) => text,
            Err(err) => {
                self.messages.error_message = format!("Failed to read '{}': {err}", diff.path);
                cx.notify();
                return;
            }
        };

        match crate::ui::diff_line_discard::discard_selected_lines_in_text(
            &diff.path,
            &diff.diff,
            &file_text,
            &selected_lines,
        ) {
            Ok(next_text) => {
                if let Err(err) = std::fs::write(&file_path, next_text) {
                    self.messages.error_message = format!("Failed to write '{}': {err}", diff.path);
                    cx.notify();
                    return;
                }
                let discarded = selected_lines.len();
                self.selection.selected_diff_lines.clear();
                self.messages.status_message = if discarded == 1 {
                    format!("Discarded 1 selected line from '{}'.", diff.path)
                } else {
                    format!("Discarded {discarded} selected lines from '{}'.", diff.path)
                };
                self.messages.error_message.clear();
                self.refresh_file_diff(diff.path);
            }
            Err(err) => {
                self.messages.error_message = format!("Failed to discard selected lines: {err}");
            }
        }
        cx.notify();
    }

    /// Re-fetch the diff for a single file in the working directory.
    pub fn refresh_file_diff(&mut self, file_path: String) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            return;
        };
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let fp = file_path.clone();
        thread::spawn(move || {
            let res = git.get_file_diff(&path, &fp).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::FileDiffRefreshed(fp, res));
        });
    }

    pub fn select_commit(&mut self, oid: String, cx: &mut Context<Self>) {
        if self.repo.comparison.is_some() {
            self.selection.selected_commit = Some(oid);
            cx.notify();
            return;
        }

        let already_selected = self.selection.selected_commit.as_deref() == Some(oid.as_str());
        if already_selected && self.selection.commit_diffs.is_some() {
            return;
        }

        self.selection.selected_commit = Some(oid.clone());
        self.selection.selected_commit_file = None;
        self.selection.commit_diffs = None;
        self.load_commit_diff(oid);
        cx.notify();
    }

    pub(crate) fn close_history_context_menu(&mut self) {}

    pub(crate) fn handle_history_context_menu_action_for_oid(
        &mut self,
        oid: String,
        action: HistoryContextMenuAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            HistoryContextMenuAction::CheckoutCommit => {
                let short = short_commit_label(&oid).to_string();
                self.run_commit_repo_action(
                    oid,
                    "Checkout commit".to_string(),
                    format!("Checked out commit {short}."),
                    GitClient::checkout_commit,
                    cx,
                );
            }
            HistoryContextMenuAction::RevertChangesInCommit => {
                let short = short_commit_label(&oid).to_string();
                self.run_commit_repo_action(
                    oid,
                    "Revert commit".to_string(),
                    format!("Reverted commit {short}."),
                    GitClient::revert_commit,
                    cx,
                );
            }
            HistoryContextMenuAction::CherryPickCommit => {
                let short = short_commit_label(&oid).to_string();
                self.repo.pending_cherry_pick_oid = Some(oid);
                self.nav.show_branch_selector = true;
                self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                self.nav.show_repo_selector = false;
                self.nav.show_network_dropdown = false;
                self.filters.branch_filter_text.clear();
                self.branch_filter_cursor = 0;
                self.messages.status_message =
                    format!("Choose a branch to cherry-pick {short} into.");
                self.messages.error_message.clear();
            }
            HistoryContextMenuAction::CopySha => {
                cx.write_to_clipboard(ClipboardItem::new_string(oid.clone()));
                self.messages.status_message = format!("Copied SHA {}.", short_commit_label(&oid));
                self.messages.error_message.clear();
            }
            HistoryContextMenuAction::CopyDiff => self.copy_commit_diff(&oid, cx),
            HistoryContextMenuAction::CopyTag => self.copy_commit_tags(&oid, cx),
            HistoryContextMenuAction::ViewOnGitHub => self.view_commit_on_github(&oid),
            HistoryContextMenuAction::CreateBranchFromCommit => {
                let short = short_commit_label(&oid);
                self.repo.new_branch_name = format!("branch-{short}");
                self.new_branch_cursor = self.repo.new_branch_name.len();
                self.new_branch_selection = None;
                self.repo.new_branch_start_point = Some(oid);
                self.nav.active_dialog = ActiveDialog::CreateBranch;
                self.messages.error_message.clear();
            }
            HistoryContextMenuAction::CreateTag => {
                self.repo.new_branch_name.clear();
                self.new_branch_cursor = 0;
                self.new_branch_selection = None;
                self.nav.active_dialog = ActiveDialog::CreateTag { target_oid: oid };
                self.messages.error_message.clear();
            }
            HistoryContextMenuAction::DeleteTag => {
                let tags = self.commit_tags_for_oid(&oid);
                if let [tag_name] = tags.as_slice() {
                    self.nav.active_dialog = ActiveDialog::DeleteTag {
                        tag_name: tag_name.clone(),
                    };
                    self.messages.error_message.clear();
                } else if tags.is_empty() {
                    self.messages.error_message =
                        format!("Commit {} has no tags.", short_commit_label(&oid));
                } else {
                    self.nav.active_dialog = ActiveDialog::ChooseTagToDelete { target_oid: oid };
                    self.messages.error_message.clear();
                }
            }
            HistoryContextMenuAction::ResetToCommit => {
                if self.can_reset_to_commit(&oid) {
                    self.nav.active_dialog = ActiveDialog::ResetToCommit { target_oid: oid };
                    self.messages.error_message.clear();
                }
            }
            HistoryContextMenuAction::ReorderCommit => {}
        }

        cx.notify();
    }

    pub(crate) fn select_branch_from_selector(&mut self, name: String, cx: &mut Context<Self>) {
        if let Some(oid) = self.repo.pending_cherry_pick_oid.take() {
            self.nav.show_branch_selector = false;
            self.nav.branch_selector_mode = BranchSelectorMode::Switch;
            self.cherry_pick_commit_onto_branch(oid, name, cx);
            return;
        }

        if self.nav.branch_selector_mode == BranchSelectorMode::Merge {
            self.nav.show_branch_selector = false;
            self.nav.branch_selector_mode = BranchSelectorMode::Switch;
            self.repo.merge_target = name;
            self.merge_branch(cx);
            return;
        }

        if self.nav.branch_selector_mode == BranchSelectorMode::Rebase {
            self.nav.show_branch_selector = false;
            self.nav.branch_selector_mode = BranchSelectorMode::Switch;
            self.repo.merge_target = name;
            self.rebase_branch(cx);
            return;
        }

        if self.nav.branch_selector_mode == BranchSelectorMode::Compare {
            self.nav.show_branch_selector = false;
            self.nav.branch_selector_mode = BranchSelectorMode::Switch;
            self.compare_branch(name, cx);
            return;
        }

        if !self
            .repo
            .snapshot
            .as_ref()
            .map(|s| s.repo.current_branch == name)
            .unwrap_or(false)
        {
            self.repo.branch_target = name;
            self.switch_branch(cx);
        }

        self.nav.show_branch_selector = false;
        self.nav.branch_selector_mode = BranchSelectorMode::Switch;
        cx.notify();
    }

    pub(crate) fn handle_changes_context_action(
        &mut self,
        path: String,
        action: ChangesContextAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            ChangesContextAction::DiscardChanges => {
                self.nav.active_dialog = ActiveDialog::DiscardChanges {
                    paths: vec![path.clone()],
                };
                self.messages.error_message.clear();
            }
            ChangesContextAction::IgnoreFile => {
                self.ignore_path(&path);
            }
            ChangesContextAction::IgnoreFolder(folder) => {
                self.ignore_path(&folder);
            }
            ChangesContextAction::IgnoreExtension => {
                if let Some(ext) = std::path::Path::new(&path)
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                {
                    self.ignore_extension(&ext);
                }
            }
            ChangesContextAction::CopyFilePath => {
                if let Some(repo_path) = self.repo_path() {
                    let full = format!("{}/{}", repo_path.display(), path);
                    cx.write_to_clipboard(ClipboardItem::new_string(full));
                    self.messages.status_message = "Copied file path.".to_string();
                    self.messages.error_message.clear();
                }
            }
            ChangesContextAction::CopyRelativePath => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                self.messages.status_message = "Copied relative path.".to_string();
                self.messages.error_message.clear();
            }
            ChangesContextAction::RevealInFinder => {
                self.reveal_in_finder(&path);
            }
            ChangesContextAction::OpenInExternalEditor => {
                self.open_in_external_editor(&path);
            }
            ChangesContextAction::OpenWithDefault => {
                self.open_with_default_program(&path);
            }
            ChangesContextAction::ViewOnGitHub => {
                self.view_file_on_github(&path);
            }
        }
        cx.notify();
    }

    pub(crate) fn handle_branch_context_action(
        &mut self,
        branch_name: String,
        action: BranchContextAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            BranchContextAction::CopyName => {
                cx.write_to_clipboard(ClipboardItem::new_string(branch_name.clone()));
                self.messages.status_message = format!("Copied branch name '{branch_name}'.");
                self.messages.error_message.clear();
            }
            BranchContextAction::Delete => {
                self.nav.show_branch_selector = false;
                self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                self.nav.active_dialog = ActiveDialog::DeleteBranch { branch_name };
                self.messages.error_message.clear();
            }
            BranchContextAction::ViewOnGitHub => {
                let Some(path) = self.repo_path().map(PathBuf::from) else {
                    self.messages.error_message = "No repository selected.".to_string();
                    return;
                };

                match self.git.github_branch_url(&path, &branch_name) {
                    Ok(Some(url)) => match open_url(&url) {
                        Ok(_) => {
                            self.messages.status_message =
                                format!("Opened branch '{branch_name}' on GitHub.");
                            self.messages.error_message.clear();
                        }
                        Err(err) => {
                            self.messages.error_message =
                                format!("Failed to open branch '{branch_name}' on GitHub: {err}");
                        }
                    },
                    Ok(None) => {
                        self.messages.error_message =
                            "This repository does not have a GitHub remote URL.".to_string();
                    }
                    Err(err) => {
                        self.messages.error_message = format!(
                            "Failed to resolve GitHub URL for branch '{branch_name}': {err}"
                        );
                    }
                }
            }
            BranchContextAction::Rename => {
                self.nav.show_branch_selector = false;
                self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                self.repo.new_branch_name = branch_name.clone();
                self.new_branch_cursor = self.repo.new_branch_name.len();
                self.new_branch_selection = None;
                self.nav.active_dialog = ActiveDialog::RenameBranch {
                    old_name: branch_name,
                };
            }
        }
        cx.notify();
    }

    pub(crate) fn confirm_delete_branch(&mut self, branch_name: String, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = format!("Deleting branch '{branch_name}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let name = branch_name.clone();
        thread::spawn(move || {
            let res = git
                .delete_branch_from_current_worktree(&path, &name)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                format!("Deleted branch '{name}'"),
            ));
        });
        cx.notify();
    }

    fn run_commit_repo_action(
        &mut self,
        oid: String,
        action_label: String,
        success_message: String,
        operation: fn(&GitClient, &Path, &str) -> anyhow::Result<RepoSnapshot>,
        _cx: &mut Context<Self>,
    ) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        self.messages.status_message = format!("{action_label} {}...", short_commit_label(&oid));
        self.messages.error_message.clear();

        let tx = self.event_tx.clone();
        let action_label_for_event = action_label.clone();

        thread::spawn(move || {
            let git = GitClient::new();
            let res = operation(&git, &path, &oid).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoOperationCompleted(
                res,
                action_label_for_event,
                success_message,
            ));
        });
    }

    fn cherry_pick_commit_onto_branch(
        &mut self,
        oid: String,
        target_branch: String,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        if self.repo.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.repo.current_branch != target_branch && !snapshot.changes.is_empty()
        }) {
            self.messages.error_message =
                "Cherry-pick target needs a clean working tree before switching branches."
                    .to_string();
            cx.notify();
            return;
        }

        let short = short_commit_label(&oid).to_string();
        let action_label = "Cherry-pick commit".to_string();
        let success_message = format!("Cherry-picked commit {short} into '{target_branch}'.");
        self.messages.status_message = format!("Cherry-picking {short} into '{target_branch}'...");
        self.messages.error_message.clear();

        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let git = GitClient::new();
            let res = git
                .cherry_pick_commit_onto_branch(&path, &oid, &target_branch)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoOperationCompleted(
                res,
                action_label,
                success_message,
            ));
        });
        cx.notify();
    }

    fn copy_commit_diff(&mut self, oid: &str, cx: &mut Context<Self>) {
        if self.selection.selected_commit.as_deref() == Some(oid)
            && let Some(diffs) = self.selection.commit_diffs.as_ref()
        {
            cx.write_to_clipboard(ClipboardItem::new_string(commit_diff_clipboard_text(diffs)));
            self.messages.status_message = format!("Copied diff for {}.", short_commit_label(oid));
            self.messages.error_message.clear();
            return;
        }

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let oid = oid.to_string();
        self.messages.status_message = format!("Copying diff for {}...", short_commit_label(&oid));
        self.messages.error_message.clear();

        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let git = GitClient::new();
            let res = git
                .get_commit_diff(&path, &oid)
                .map(|diffs| commit_diff_clipboard_text(&diffs))
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitDiffCopied(oid, res));
        });

        cx.notify();
    }

    fn copy_commit_tags(&mut self, oid: &str, cx: &mut Context<Self>) {
        let tags = self.commit_tags_for_oid(oid);
        if tags.is_empty() {
            self.messages.error_message =
                format!("Commit {} has no tags.", short_commit_label(oid));
            return;
        }

        let copied = tags.join(" ");
        cx.write_to_clipboard(ClipboardItem::new_string(copied));
        self.messages.status_message = if tags.len() == 1 {
            format!("Copied tag {}.", tags[0])
        } else {
            format!("Copied {} tags.", tags.len())
        };
        self.messages.error_message.clear();
    }

    pub(crate) fn reset_to_commit(&mut self, oid: String, cx: &mut Context<Self>) {
        self.nav.active_dialog = ActiveDialog::None;
        self.nav.sidebar_tab = SidebarTab::Changes;
        self.run_commit_repo_action(
            oid.clone(),
            "Reset to commit".to_string(),
            format!("Reset to commit {}.", short_commit_label(&oid)),
            GitClient::reset_to_commit,
            cx,
        );
    }

    fn view_commit_on_github(&mut self, oid: &str) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.github_commit_url(&path, oid) {
            Ok(Some(url)) => match open_url(&url) {
                Ok(_) => {
                    self.messages.status_message =
                        format!("Opened commit {} on GitHub.", short_commit_label(oid));
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message = format!(
                        "Failed to open commit {} on GitHub: {err}",
                        short_commit_label(oid)
                    );
                }
            },
            Ok(None) => {
                self.messages.error_message =
                    "This repository does not have a GitHub remote URL.".to_string();
            }
            Err(err) => {
                self.messages.error_message = format!(
                    "Failed to resolve GitHub URL for {}: {err}",
                    short_commit_label(oid)
                );
            }
        }
    }

    fn view_file_on_github(&mut self, relative_path: &str) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.github_file_url(&path, relative_path) {
            Ok(Some(url)) => match open_url(&url) {
                Ok(_) => {
                    self.messages.status_message = format!("Opened '{}' on GitHub.", relative_path);
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message =
                        format!("Failed to open '{}' on GitHub: {err}", relative_path);
                }
            },
            Ok(None) => {
                self.messages.error_message =
                    "This repository does not have a GitHub remote URL.".to_string();
            }
            Err(err) => {
                self.messages.error_message = format!(
                    "Failed to resolve GitHub URL for '{}': {err}",
                    relative_path
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // File operations
    // ------------------------------------------------------------------

    pub(crate) fn discard_change(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.discard_change(&repo_path, relative_path) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.messages.status_message =
                    format!("Discarded changes for '{}'.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to discard changes for '{}': {err}", relative_path);
            }
        }
    }

    fn ignore_path(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };
        if Path::new(relative_path)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(".gitignore")
        {
            self.messages.error_message = "Cannot ignore .gitignore.".to_string();
            return;
        }

        let pattern = relative_path.replace('\\', "/");
        match self.git.append_gitignore_pattern(&repo_path, &pattern) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.messages.status_message = format!("Added '{}' to .gitignore.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to ignore '{}': {err}", relative_path);
            }
        }
    }

    fn ignore_extension(&mut self, ext: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let pattern = format!("*.{ext}");
        match self.git.append_gitignore_pattern(&repo_path, &pattern) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.messages.status_message = format!("Added '{}' to .gitignore.", pattern);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!("Failed to ignore '{}': {err}", pattern);
            }
        }
    }

    pub(crate) fn reveal_in_finder(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };
        let full_path = repo_path.join(relative_path);

        let result = reveal_path(&full_path);

        match result {
            Ok(_) => {
                self.messages.status_message = format!("Revealed '{}' in Finder.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to reveal '{}': {err}", relative_path);
            }
        }
    }

    fn open_in_external_editor(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let full_path = repo_path.join(relative_path);
        let configured_editor = external_command_from_env("GITSPARK_EDITOR_COMMAND")
            .or_else(|| {
                self.git
                    .read_config_value(&repo_path, "core.editor")
                    .ok()
                    .flatten()
                    .filter(|value| !value.trim().is_empty())
            })
            .or_else(|| external_command_from_env("VISUAL"))
            .or_else(|| external_command_from_env("EDITOR"));

        let result = if let Some(editor_cmd) = configured_editor {
            spawn_shell_path_command(&editor_cmd, &full_path)
        } else {
            open::that_detached(&full_path)
        };

        match result {
            Ok(_) => {
                self.messages.status_message =
                    format!("Opened '{}' in external editor.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!(
                    "Failed to open '{}' in external editor: {err}",
                    relative_path
                );
            }
        }
    }

    pub(crate) fn open_with_default_program(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path() else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let full_path = repo_path.join(relative_path);
        match open_with_default_program(&full_path) {
            Ok(_) => {
                self.messages.status_message =
                    format!("Opened '{relative_path}' with the default program.");
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to open '{relative_path}' with default program: {err}");
            }
        }
    }

    pub(crate) fn open_submodule_repository(
        &mut self,
        relative_path: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_path) = self.repo_path() else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let full_path = repo_path.join(relative_path);
        if !full_path.is_dir() {
            self.messages.error_message =
                format!("Submodule '{}' is not checked out locally.", relative_path);
            return;
        }

        self.open_repo_with_notify(full_path, cx);
    }

    // ------------------------------------------------------------------
    // Settings persistence
    // ------------------------------------------------------------------

    fn add_recent_repo(&mut self, path: PathBuf) {
        push_recent_repo(&mut self.settings, path);
        self.persist_settings();
    }

    pub fn persist_settings(&mut self) {
        if let Err(err) = save_settings(&self.settings) {
            self.messages.error_message = format!("Failed to save settings: {err}");
        }
    }

    /// Apply an appearance preference, persist it, and re-theme immediately.
    ///
    /// This is the whole of light mode at the app level: our own tokens
    /// re-resolve through `theme::resolve`, and gpui-component's global theme
    /// is pointed at the same answer so its stock widgets don't stay on the
    /// old palette (design.md §13 — a swap, never a fork).
    /// `window` is optional so the automation channel — which has no Window
    /// in scope — can drive this exactly like a click does.
    pub fn set_appearance(
        &mut self,
        pref: theme::Appearance,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        theme::set_appearance(pref);
        let system_appearance = match window.as_ref() {
            Some(window) => window.appearance(),
            None => cx.window_appearance(),
        };
        let dark = theme::resolve(matches!(
            system_appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ));

        self.settings.appearance = Some(pref.as_str().to_string());
        self.persist_settings();

        gpui_component::Theme::change(
            if dark {
                gpui_component::ThemeMode::Dark
            } else {
                gpui_component::ThemeMode::Light
            },
            window,
            cx,
        );
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Snapshot adoption
    // ------------------------------------------------------------------

    fn adopt_snapshot(&mut self, snapshot: RepoSnapshot) {
        let previous_commit = self.selection.selected_commit.clone();
        let previous_commit_file = self.selection.selected_commit_file.clone();
        let previous_branch = self
            .repo
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.repo.current_branch.clone());
        let previous_comparison = self.repo.comparison.clone();
        let previous_operation_target = self
            .repo
            .operation
            .as_ref()
            .and_then(|operation| operation.target_branch.clone());
        let current_branch = snapshot.repo.current_branch.clone();
        let has_stash = snapshot.stash_count > 0;
        let changed_paths: Vec<String> = snapshot
            .changes
            .iter()
            .map(|change| change.path.clone())
            .collect();
        self.close_history_context_menu();
        self.repo.comparison = previous_comparison.filter(|comparison| {
            previous_branch.as_deref() == Some(current_branch.as_str())
                && snapshot
                    .branches
                    .iter()
                    .any(|branch| branch.name == comparison.target_branch)
        });
        self.repo.operation = self
            .git
            .operation_state(&snapshot.repo.path)
            .unwrap_or(None)
            .map(|mut operation| {
                if operation.target_branch.is_none() {
                    operation.target_branch = previous_operation_target;
                }
                operation
            });
        let next_selected_change = snapshot.changes.first().map(|change| change.path.clone());
        if self.selection.selected_change.as_ref() != next_selected_change.as_ref() {
            self.selection.selected_diff_lines.clear();
        }
        self.selection.selected_change = next_selected_change;
        self.repo.branch_target = current_branch;
        self.repo.merge_target = snapshot
            .branches
            .iter()
            .find(|branch| !branch.is_current && !branch.is_remote)
            .map(|branch| branch.name.clone())
            .unwrap_or_default();
        self.load_identity(&snapshot.repo.path);
        self.load_remote_settings(&snapshot.repo.path);
        self.load_ignored_files_settings(&snapshot.repo.path);
        self.ensure_repo_watch(&snapshot.repo.path);
        self.repo.has_stash = has_stash;
        if has_stash {
            self.repo.stash_files = self
                .git
                .latest_stash_files(&snapshot.repo.path)
                .unwrap_or_default();
        } else {
            self.repo.stash_files.clear();
        }
        self.repo.snapshot = Some(snapshot);
        self.reconcile_commit_inclusions(&changed_paths);

        self.selection.commit_diffs = None;

        if let Some(comparison) = self.repo.comparison.as_ref() {
            self.selection.selected_commit =
                comparison.commits.first().map(|commit| commit.oid.clone());
            self.selection.selected_commit_file = previous_commit_file
                .filter(|path| comparison.diffs.iter().any(|diff| diff.path == *path))
                .or_else(|| comparison.diffs.first().map(|diff| diff.path.clone()));
        } else {
            let next_selected_commit = self.repo.snapshot.as_ref().and_then(|repo| {
                previous_commit
                    .filter(|oid| repo.history.iter().any(|commit| commit.oid == *oid))
                    .or_else(|| repo.history.first().map(|commit| commit.oid.clone()))
            });

            self.selection.selected_commit = next_selected_commit.clone();
            self.selection.selected_commit_file = None;

            if let Some(oid) = next_selected_commit {
                self.load_commit_diff(oid);
            }
        }
    }

    fn refresh_git_operation_state(&mut self, target_hint: Option<String>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.repo.operation = None;
            return;
        };

        self.repo.operation =
            self.git
                .operation_state(&path)
                .unwrap_or(None)
                .map(|mut operation| {
                    if operation.target_branch.is_none() {
                        operation.target_branch = target_hint;
                    }
                    operation
                });
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    pub fn repo_path(&self) -> Option<&Path> {
        self.repo
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.repo.path.as_path())
    }

    pub(crate) fn repo_has_github_remote(&self) -> bool {
        let Some(path) = self.repo_path() else {
            return false;
        };

        self.git
            .github_repository_url(path)
            .ok()
            .flatten()
            .is_some()
    }

    pub(crate) fn commit_tags_for_oid(&self, oid: &str) -> Vec<String> {
        self.repo
            .snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .history
                    .iter()
                    .find(|commit| commit.oid == oid)
                    .map(|commit| commit.tags.clone())
            })
            .unwrap_or_default()
    }

    pub(crate) fn tag_name_exists(&self, tag_name: &str) -> bool {
        let tag_name = tag_name.trim();
        !tag_name.is_empty()
            && self
                .repo
                .snapshot
                .as_ref()
                .map(|snapshot| {
                    snapshot.tags.iter().any(|tag| tag == tag_name)
                        || snapshot
                            .history
                            .iter()
                            .flat_map(|commit| commit.tags.iter())
                            .any(|tag| tag == tag_name)
                })
                .unwrap_or(false)
    }

    pub(crate) fn branch_name_exists(&self, branch_name: &str) -> bool {
        let branch_name = branch_name.trim();
        !branch_name.is_empty()
            && self
                .repo
                .snapshot
                .as_ref()
                .map(|snapshot| {
                    snapshot
                        .branches
                        .iter()
                        .any(|branch| branch.name == branch_name)
                })
                .unwrap_or(false)
    }

    pub(crate) fn create_branch_validation_message(&self) -> Option<String> {
        let proposed_name = self.repo.new_branch_name.trim();
        if proposed_name.is_empty() {
            return Some("Type a branch name.".to_string());
        }
        let branch_name = sanitized_ref_name(proposed_name);
        if branch_name.is_empty() {
            return Some(format!("{proposed_name} is not a valid name."));
        }
        if self.branch_name_exists(&branch_name) {
            return Some(format!("A branch named {branch_name} already exists."));
        }
        if branch_name != proposed_name {
            return Some(format!(
                "Will be created as {branch_name}. Spaces and invalid characters have been replaced by hyphens."
            ));
        }
        None
    }

    pub(crate) fn can_create_branch_from_dialog(&self) -> bool {
        let proposed_name = self.repo.new_branch_name.trim();
        let branch_name = sanitized_ref_name(proposed_name);
        !proposed_name.is_empty()
            && !branch_name.is_empty()
            && !self.branch_name_exists(&branch_name)
    }

    pub(crate) fn rename_branch_validation_message(&self, old_name: &str) -> Option<String> {
        let proposed_name = self.repo.new_branch_name.trim();
        if proposed_name.is_empty() {
            return Some("Type a new branch name.".to_string());
        }
        let branch_name = sanitized_ref_name(proposed_name);
        if branch_name.is_empty() {
            return Some(format!("{proposed_name} is not a valid name."));
        }
        if branch_name == old_name {
            return Some("Type a different branch name.".to_string());
        }
        if self.branch_name_exists(&branch_name) {
            return Some(format!("A branch named {branch_name} already exists."));
        }
        if branch_name != proposed_name {
            return Some(format!(
                "Will be renamed as {branch_name}. Spaces and invalid characters have been replaced by hyphens."
            ));
        }
        None
    }

    pub(crate) fn can_rename_branch_from_dialog(&self, old_name: &str) -> bool {
        let proposed_name = self.repo.new_branch_name.trim();
        let branch_name = sanitized_ref_name(proposed_name);
        !proposed_name.is_empty()
            && !branch_name.is_empty()
            && branch_name != old_name
            && !self.branch_name_exists(&branch_name)
    }

    pub(crate) fn create_tag_validation_message(&self) -> Option<String> {
        let tag_name = self.repo.new_branch_name.trim();
        if tag_name.is_empty() {
            return Some("Type a tag name.".to_string());
        }
        if let Some(message) = tag_name_length_validation_message(tag_name) {
            return Some(message);
        }
        if self.tag_name_exists(tag_name) {
            return Some(format!("A tag named {tag_name} already exists."));
        }
        None
    }

    pub(crate) fn can_undo_last_commit(&self) -> bool {
        self.nav.undo_commit.is_some()
            && self
                .repo
                .snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot
                        .repo
                        .head_oid
                        .as_ref()
                        .and_then(|head_oid| {
                            snapshot
                                .history
                                .iter()
                                .find(|commit| &commit.oid == head_oid)
                        })
                        .or_else(|| snapshot.history.first())
                })
                .map(|commit| commit.tags.is_empty())
                .unwrap_or(false)
    }

    pub(crate) fn can_reset_to_commit(&self, oid: &str) -> bool {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            return false;
        };
        let Some(index) = snapshot.history.iter().position(|commit| commit.oid == oid) else {
            return false;
        };
        if index == 0 {
            return false;
        }
        snapshot.repo.remote_name.is_none() || index <= snapshot.repo.ahead
    }

    fn reconcile_commit_inclusions(&mut self, changed_paths: &[String]) {
        if self.commit.include_all {
            self.commit.included_files.clear();
            return;
        }

        self.commit.included_files.retain(|path| {
            changed_paths
                .iter()
                .any(|changed_path| changed_path == path)
        });

        if !changed_paths.is_empty() && self.commit.included_files.len() == changed_paths.len() {
            self.commit.include_all = true;
            self.commit.included_files.clear();
        }
    }

    pub(crate) fn commit_file_count(&self) -> usize {
        self.repo
            .snapshot
            .as_ref()
            .map(|snapshot| {
                if self.commit.include_all {
                    snapshot.changes.len()
                } else {
                    self.commit.included_files.len()
                }
            })
            .unwrap_or(0)
    }

    fn included_commit_changes(&self) -> Vec<&ChangeEntry> {
        let Some(snapshot) = &self.repo.snapshot else {
            return Vec::new();
        };

        if self.commit.include_all {
            snapshot.changes.iter().collect()
        } else {
            snapshot
                .changes
                .iter()
                .filter(|change| self.commit.included_files.contains(&change.path))
                .collect()
        }
    }

    fn included_commit_paths(&self) -> Option<Vec<String>> {
        if self.commit.include_all {
            None
        } else {
            Some(
                self.included_commit_changes()
                    .into_iter()
                    .map(|change| change.path.clone())
                    .collect(),
            )
        }
    }

    pub(crate) fn default_commit_summary(&self) -> Option<String> {
        if !self.commit.summary.trim().is_empty() {
            return None;
        }

        let changes = self.included_commit_changes();
        if changes.len() != 1 {
            return None;
        }

        Some(default_commit_summary_for_change(changes[0]))
    }

    pub(crate) fn can_commit(&self) -> bool {
        !self.commit.summary.trim().is_empty()
            && self.commit_file_count() > 0
            && self.identity_is_configured()
            && !self.commit.ai_in_flight
    }

    fn identity_is_configured(&self) -> bool {
        self.missing_identity_message().is_none()
    }

    pub(crate) fn missing_identity_message(&self) -> Option<&'static str> {
        let missing_name = self.repo.identity.user_name.trim().is_empty();
        let missing_email = self.repo.identity.user_email.trim().is_empty();

        if !missing_name && !git_author_name_is_valid(&self.repo.identity.user_name) {
            return Some(INVALID_GIT_AUTHOR_NAME_MESSAGE);
        }

        match (missing_name, missing_email) {
            (true, true) => Some("Configure your Git name and email before committing."),
            (true, false) => Some("Configure your Git name before committing."),
            (false, true) => Some("Configure your Git email before committing."),
            (false, false) => None,
        }
    }

    pub(crate) fn identity_settings_focus_field(&self) -> SettingsField {
        identity_settings_focus_field_for(&self.repo.identity)
    }

    pub(crate) fn open_identity_settings_from_warning(&mut self, cx: &mut Context<Self>) {
        let field = self.identity_settings_focus_field();
        if self.repo.use_local_identity {
            self.open_repository_settings_modal(
                Some(crate::ui::ui_state::SettingsSection::Git),
                cx,
            );
        } else {
            self.open_global_settings_modal(Some(crate::ui::ui_state::SettingsSection::Git), cx);
        }
        self.settings_modal.active_field = Some(field);
        self.set_settings_field_cursor(field, self.settings_field_value(field).len());
    }

    #[allow(dead_code)]
    pub fn selected_diff(&self) -> Option<&DiffEntry> {
        let snapshot = self.repo.snapshot.as_ref()?;
        let selected_change = self.selection.selected_change.as_ref()?;
        snapshot
            .diffs
            .iter()
            .find(|diff| &diff.path == selected_change)
    }
}
