use super::helpers::*;
use super::*;

impl Render for GitSparkApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Capture window bounds and persist when they change.
        // Skip the first 10 renders to let GPUI settle the window position.
        self.render_count = self.render_count.saturating_add(1);
        if self.render_count > 10 {
            let bounds = window.bounds();
            let new_x = bounds.origin.x / px(1.0);
            let new_y = bounds.origin.y / px(1.0);
            let new_w = bounds.size.width / px(1.0);
            let new_h = bounds.size.height / px(1.0);
            let ws = &self.settings.window_size;
            let changed = !ws.has_position
                || (ws.x - new_x).abs() > 1.0
                || (ws.y - new_y).abs() > 1.0
                || (ws.width - new_w).abs() > 1.0
                || (ws.height - new_h).abs() > 1.0;
            if changed {
                self.settings.window_size.x = new_x;
                self.settings.window_size.y = new_y;
                self.settings.window_size.width = new_w;
                self.settings.window_size.height = new_h;
                self.settings.window_size.has_position = true;
                self.settings.window_size.display_id = window.display(cx).map(|d| d.id().into());
                // Off-thread and debounced. This runs on essentially every
                // frame of a resize or drag; a blocking write here churned the
                // disk and stalled the UI for the whole gesture.
                self.queue_window_size_write();
            }
        }

        // Refresh git changes when window gains focus
        let is_active = window.is_window_active();
        if is_active && !self.was_window_active && self.repo.snapshot.is_some() {
            self.request_repo_refresh(RepoRefreshReason::Focus, cx);
        }
        self.was_window_active = is_active;

        // Clamp cursors to valid positions (e.g. after AI fill or clear)
        self.summary_cursor = self.summary_cursor.min(self.commit.summary.len());
        self.description_cursor = self.description_cursor.min(self.commit.body.len());
        let git_identity = self.active_git_settings_identity();
        let git_user_name_len = git_identity.user_name.len();
        let git_user_email_len = git_identity.user_email.len();
        let git_default_branch_len = git_identity.default_branch.as_deref().unwrap_or("").len();
        self.settings_modal.git_user_name_cursor = self
            .settings_modal
            .git_user_name_cursor
            .min(git_user_name_len);
        self.settings_modal.git_user_email_cursor = self
            .settings_modal
            .git_user_email_cursor
            .min(git_user_email_len);
        self.settings_modal.git_default_branch_cursor = self
            .settings_modal
            .git_default_branch_cursor
            .min(git_default_branch_len);
        self.settings_modal.ai_model_cursor = self
            .settings_modal
            .ai_model_cursor
            .min(self.settings.ai.model.len());
        self.settings_modal.ai_api_key_cursor = self
            .settings_modal
            .ai_api_key_cursor
            .min(self.settings.ai.api_key.len());
        self.settings_modal.ai_system_prompt_cursor = self
            .settings_modal
            .ai_system_prompt_cursor
            .min(self.settings.ai.system_prompt.len());
        self.settings_modal.openrouter_model_filter_cursor = self
            .settings_modal
            .openrouter_model_filter_cursor
            .min(self.filters.openrouter_model_filter.len());

        if self.pending_summary_focus {
            self.pending_summary_focus = false;
            window.focus(&self.summary_focus);
        }

        let summary_focused = self.summary_focus.is_focused(window);
        let description_focused = self.description_focus.is_focused(window);
        let branch_filter_focused = self.branch_filter_focus.is_focused(window);
        let worktree_filter_focused = self.worktree_filter_focus.is_focused(window);
        let repo_filter_focused = self.repo_filter_focus.is_focused(window);

        // Clamp filter cursors
        self.branch_filter_cursor = self
            .branch_filter_cursor
            .min(self.filters.branch_filter_text.len());
        self.repo_filter_cursor = self
            .repo_filter_cursor
            .min(self.filters.repo_filter_text.len());

        // Build toolbar parts separately — they go into the resizable columns
        let (toolbar_left, toolbar_right) = self.render_toolbar_parts(cx);

        // Left column: repo toolbar section + sidebar (or repo selector)
        let left_column = v_flex().size_full().min_h_0().child(toolbar_left).child(
            if self.nav.show_repo_selector {
                repo_selector::render_repo_selector_panel(self, repo_filter_focused, cx)
                    .into_any_element()
            } else {
                self.render_sidebar(summary_focused, description_focused, cx)
                    .into_any_element()
            },
        );

        // Right column: branch + network toolbar sections + workspace
        // Branch selector overlay lives inside the right column so it aligns naturally
        let show_network_overlay = self.nav.show_network_dropdown
            && self
                .repo
                .snapshot
                .as_ref()
                .map(NetworkAction::from_snapshot)
                .is_some_and(|action| matches!(action, NetworkAction::Pull | NetworkAction::Push));

        let right_column = div()
            .size_full()
            .min_h_0()
            .relative()
            .child(
                v_flex()
                    .size_full()
                    .min_h_0()
                    .child(toolbar_right)
                    .child(self.render_workspace(cx.entity().clone(), cx)),
            )
            .children(if self.nav.show_worktree_selector {
                Some(render_worktree_overlay(self, worktree_filter_focused, cx))
            } else {
                None
            })
            .children(if self.nav.show_branch_selector {
                Some(branch_selector::render_branch_selector_overlay(
                    self,
                    branch_filter_focused,
                    cx,
                ))
            } else {
                None
            })
            .children(if show_network_overlay {
                Some(dialogs::render_network_dropdown_overlay(self, cx))
            } else {
                None
            });

        // Apply zoom level
        let zoom_factor = self.rem_size / DEFAULT_REM_SIZE;
        theme::set_zoom(zoom_factor);
        window.set_rem_size(px(self.rem_size));

        // macOS titlebar spacer (traffic lights sit here)
        let titlebar_height = if cfg!(target_os = "macos") { 38.0 } else { 0.0 };

        let titlebar_spacer = {
            // The update indicator lives at the top right of this strip, the
            // way Zed does it — the traffic lights own the left, and this is
            // otherwise dead space.
            let spacer = h_flex()
                .id("window-titlebar-spacer")
                .w_full()
                .h(px(titlebar_height))
                .flex_shrink_0()
                .items_center()
                .justify_end()
                .pr(theme::z(theme::SPACE_6))
                .child(crate::ui::update_indicator::render(&self.update_state));
            #[cfg(target_os = "macos")]
            let spacer = spacer.on_click(|event: &ClickEvent, window: &mut Window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            });
            spacer.bg(theme::panel_bg())
        };

        let mut root = v_flex()
            .size_full()
            .relative()
            .bg(theme::bg())
            .font_family(".SystemUIFont")
            .text_size(theme::z(theme::FONT_SIZE))
            .child(titlebar_spacer) // slightly lighter than bg for titlebar strip
            .child(
                div().w_full().flex_1().min_h_0().child(
                    h_resizable("main-panels")
                        .child(
                            resizable_panel()
                                .size(px(260.0))
                                .size_range(px(200.0)..px(400.0))
                                .child(left_column),
                        )
                        .child(resizable_panel().child(right_column)),
                ),
            )
            .child(self.render_status_bar());

        if self.nav.show_settings {
            root = root.child(settings_modal::render_settings_modal(self, window, cx));
        }

        // Dialogs
        if self.nav.active_dialog != ActiveDialog::None {
            root = root.child(dialogs::render_active_dialog(self, window, cx));
        }

        if let Some(context_menu) =
            changes_context_menu::render_changes_context_menu_overlay(self, window, cx)
        {
            root = root.child(context_menu);
        }

        root = root
            .on_action(cx.listener(Self::handle_menu_open_repository))
            .on_action(cx.listener(Self::handle_menu_new_repository))
            .on_action(cx.listener(Self::handle_menu_clone_repository))
            .on_action(cx.listener(Self::handle_menu_show_settings))
            .on_action(cx.listener(Self::handle_menu_show_changes))
            .on_action(cx.listener(Self::handle_menu_show_history))
            .on_action(cx.listener(Self::handle_menu_show_repository_list))
            .on_action(cx.listener(Self::handle_menu_show_branches_list))
            .on_action(cx.listener(Self::handle_menu_go_to_summary))
            .on_action(cx.listener(Self::handle_menu_show_stashed_changes))
            .on_action(cx.listener(Self::handle_menu_fetch))
            .on_action(cx.listener(Self::handle_menu_pull))
            .on_action(cx.listener(Self::handle_menu_push))
            .on_action(cx.listener(Self::handle_menu_publish_repository))
            .on_action(cx.listener(Self::handle_menu_open_external_editor))
            .on_action(cx.listener(Self::handle_menu_open_in_terminal))
            .on_action(cx.listener(Self::handle_menu_show_in_finder))
            .on_action(cx.listener(Self::handle_menu_view_on_github))
            .on_action(cx.listener(Self::handle_menu_repository_settings))
            .on_action(cx.listener(Self::handle_menu_new_branch))
            .on_action(cx.listener(Self::handle_menu_rename_branch))
            .on_action(cx.listener(Self::handle_menu_delete_branch))
            .on_action(cx.listener(Self::handle_menu_update_from_default_branch))
            .on_action(cx.listener(Self::handle_menu_compare_branch))
            .on_action(cx.listener(Self::handle_menu_merge_branch))
            .on_action(cx.listener(Self::handle_menu_rebase_branch))
            .on_action(cx.listener(Self::handle_menu_compare_on_github))
            .on_action(cx.listener(Self::handle_menu_view_branch_on_github))
            .on_action(cx.listener(Self::handle_menu_discard_all_changes))
            .on_action(cx.listener(Self::handle_menu_stash_changes))
            .on_action(cx.listener(Self::handle_menu_zoom_in))
            .on_action(cx.listener(Self::handle_menu_zoom_out))
            .on_action(cx.listener(Self::handle_menu_zoom_reset));

        crate::install_native_menus(cx, self.native_menu_availability());

        root
    }
}

impl GitSparkApp {
    pub(crate) fn native_menu_availability(&self) -> crate::MenuAvailability {
        crate::MenuAvailability {
            has_repository: self.menu_has_repository(),
            fetch: self.menu_can_fetch(),
            pull: self.menu_can_pull(),
            push: self.menu_can_push(),
            publish_repository: self.menu_can_publish_repository(),
            view_repository_on_github: self.menu_can_view_repository_on_github(),
            create_branch: self.menu_can_create_branch(),
            modify_current_branch: self.menu_can_modify_current_branch(),
            compare_on_github: self.menu_can_compare_on_github(),
            change_worktree: self.menu_can_change_worktree(),
        }
    }

    fn menu_has_repository(&self) -> bool {
        self.repo.snapshot.is_some()
    }

    fn menu_has_named_branch(&self) -> bool {
        self.repo.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.repo.head_oid.is_some() && snapshot.repo.current_branch != "detached HEAD"
        })
    }

    fn menu_no_active_operation(&self) -> bool {
        self.repo.operation.is_none()
    }

    fn menu_has_remote(&self) -> bool {
        self.repo
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.repo.remote_name.is_some())
    }

    fn menu_can_fetch(&self) -> bool {
        self.menu_has_repository() && self.menu_has_remote() && self.menu_no_active_operation()
    }

    fn menu_can_pull(&self) -> bool {
        self.menu_can_fetch() && self.menu_has_named_branch()
    }

    fn menu_can_push(&self) -> bool {
        self.menu_can_fetch() && self.menu_has_named_branch()
    }

    fn menu_can_publish_repository(&self) -> bool {
        self.repo.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.repo.remote_name.is_none() && self.menu_no_active_operation()
        })
    }

    fn menu_can_view_repository_on_github(&self) -> bool {
        self.menu_has_repository() && self.repo_has_github_remote()
    }

    fn menu_can_create_branch(&self) -> bool {
        self.menu_has_repository() && self.menu_no_active_operation()
    }

    fn menu_can_modify_current_branch(&self) -> bool {
        self.menu_has_named_branch() && self.menu_no_active_operation()
    }

    fn menu_can_compare_on_github(&self) -> bool {
        self.menu_can_modify_current_branch() && self.repo_has_github_remote()
    }

    fn menu_can_change_worktree(&self) -> bool {
        self.repo
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| self.menu_no_active_operation() && !snapshot.changes.is_empty())
    }

    pub fn menu_open_repository(&mut self, cx: &mut Context<Self>) {
        self.open_repo_dialog(cx);
    }

    pub fn menu_new_repository(&mut self, cx: &mut Context<Self>) {
        self.open_create_repository_dialog(cx);
    }

    pub fn menu_clone_repository(&mut self, cx: &mut Context<Self>) {
        self.open_clone_repository_dialog(cx);
    }

    pub fn menu_show_settings(&mut self, cx: &mut Context<Self>) {
        self.open_global_settings_modal(None, cx);
        cx.notify();
    }

    pub fn menu_show_changes(&mut self, cx: &mut Context<Self>) {
        self.nav.sidebar_tab = SidebarTab::Changes;
        cx.notify();
    }

    pub fn menu_show_history(&mut self, cx: &mut Context<Self>) {
        self.nav.sidebar_tab = SidebarTab::History;
        cx.notify();
    }

    pub fn menu_show_repository_list(&mut self, cx: &mut Context<Self>) {
        self.nav.show_repo_selector = true;
        self.nav.show_branch_selector = false;
        self.nav.branch_selector_mode = BranchSelectorMode::Switch;
        self.nav.show_network_dropdown = false;
        self.close_history_context_menu();
        cx.notify();
    }

    pub fn menu_show_branches_list(&mut self, cx: &mut Context<Self>) {
        if self.repo.snapshot.is_none() {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        }

        self.nav.show_branch_selector = true;
        self.nav.branch_selector_mode = BranchSelectorMode::Switch;
        self.nav.show_repo_selector = false;
        self.nav.show_network_dropdown = false;
        self.repo.pending_cherry_pick_oid = None;
        self.close_history_context_menu();
        cx.notify();
    }

    pub fn menu_go_to_summary(&mut self, cx: &mut Context<Self>) {
        self.nav.sidebar_tab = SidebarTab::Changes;
        self.summary_cursor = self.commit.summary.len();
        self.summary_selection = None;
        self.pending_summary_focus = true;
        cx.notify();
    }

    pub fn menu_show_stashed_changes(&mut self, cx: &mut Context<Self>) {
        if matches!(self.nav.active_dialog, ActiveDialog::RestoreStash) {
            self.nav.active_dialog = ActiveDialog::None;
            cx.notify();
            return;
        }

        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        if snapshot.stash_count == 0 {
            self.messages.error_message = "There are no stashed changes.".to_string();
            cx.notify();
            return;
        }

        self.show_restore_stash_dialog(cx);
    }

    pub fn menu_fetch(&mut self, cx: &mut Context<Self>) {
        if !self.menu_can_fetch() {
            self.messages.error_message =
                "Fetch requires a selected repository with a remote.".to_string();
            cx.notify();
            return;
        }

        self.fetch_origin(cx);
    }

    pub fn menu_pull(&mut self, cx: &mut Context<Self>) {
        if !self.menu_can_pull() {
            self.messages.error_message =
                "Pull requires a selected branch with a remote.".to_string();
            cx.notify();
            return;
        }

        self.pull_origin(cx);
    }

    pub fn menu_push(&mut self, cx: &mut Context<Self>) {
        if !self.menu_can_push() {
            self.messages.error_message =
                "Push requires a selected branch with a remote.".to_string();
            cx.notify();
            return;
        }

        self.push_origin(cx);
    }

    pub fn menu_publish_repository(&mut self, cx: &mut Context<Self>) {
        if !self.menu_can_publish_repository() {
            self.messages.error_message =
                "Publish requires a selected repository without a remote.".to_string();
            cx.notify();
            return;
        }

        self.run_network_action(NetworkAction::PublishRepository, cx);
    }

    pub fn menu_open_external_editor(&mut self, cx: &mut Context<Self>) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let configured_editor = self
            .git
            .read_config_value(&repo_path, "core.editor")
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env::var("VISUAL")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .or_else(|| {
                env::var("EDITOR")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            });

        let result = if let Some(editor_cmd) = configured_editor {
            Command::new("sh")
                .arg("-lc")
                .arg(format!(
                    "{} {}",
                    editor_cmd,
                    shell_escape(&repo_path.to_string_lossy())
                ))
                .spawn()
                .map(|_| ())
        } else {
            open::that_detached(&repo_path)
        };

        match result {
            Ok(_) => {
                self.messages.status_message = "Opened repository in external editor.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to open repository in external editor: {err}");
            }
        }
        cx.notify();
    }

    pub fn menu_open_in_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        #[cfg(target_os = "macos")]
        let result = Command::new("open")
            .arg("-a")
            .arg("Terminal")
            .arg(&repo_path)
            .spawn()
            .map(|_| ());

        #[cfg(not(target_os = "macos"))]
        let result = open::that_detached(&repo_path);

        match result {
            Ok(_) => {
                self.messages.status_message = "Opened repository in Terminal.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to open repository in Terminal: {err}");
            }
        }
        cx.notify();
    }

    pub fn menu_show_in_finder(&mut self, cx: &mut Context<Self>) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        match reveal_path(&repo_path) {
            Ok(_) => {
                self.messages.status_message = "Revealed repository in Finder.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to reveal repository in Finder: {err}");
            }
        }
        cx.notify();
    }

    pub fn menu_view_on_github(&mut self, cx: &mut Context<Self>) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        match self.git.github_repository_url(&repo_path) {
            Ok(Some(url)) => match open_url(&url) {
                Ok(_) => {
                    self.messages.status_message = "Opened repository page on GitHub.".to_string();
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message =
                        format!("Failed to open repository on GitHub: {err}");
                }
            },
            Ok(None) => {
                self.messages.error_message =
                    "This repository does not have a GitHub remote URL.".to_string();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to resolve repository GitHub URL: {err}");
            }
        }
        cx.notify();
    }

    pub fn menu_repository_settings(&mut self, cx: &mut Context<Self>) {
        if self.repo.snapshot.is_none() {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        }

        self.open_repository_settings_modal(Some(crate::ui::ui_state::SettingsSection::Remote), cx);
        cx.notify();
    }

    pub fn menu_new_branch(&mut self, cx: &mut Context<Self>) {
        if self.repo.snapshot.is_none() {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        }

        self.repo.new_branch_name = self.filters.branch_filter_text.clone();
        self.new_branch_cursor = self.repo.new_branch_name.len();
        self.new_branch_selection = None;
        self.repo.new_branch_start_point = None;
        self.nav.show_branch_selector = false;
        self.nav.branch_selector_mode = BranchSelectorMode::Switch;
        self.nav.active_dialog = ActiveDialog::CreateBranch;
        cx.notify();
    }

    pub fn menu_rename_current_branch(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let branch_name = snapshot.repo.current_branch.clone();
        self.repo.new_branch_name = branch_name.clone();
        self.new_branch_cursor = self.repo.new_branch_name.len();
        self.new_branch_selection = None;
        self.nav.active_dialog = ActiveDialog::RenameBranch {
            old_name: branch_name,
        };
        self.messages.error_message.clear();
        cx.notify();
    }

    pub fn menu_delete_current_branch(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let branch_name = snapshot.repo.current_branch.clone();
        let default_branch = self.default_branch_name();
        if branch_name == default_branch || branch_name == "main" || branch_name == "master" {
            self.messages.error_message =
                "Cannot delete the default branch from the Branch menu.".to_string();
            cx.notify();
            return;
        }

        if snapshot
            .branches
            .iter()
            .filter(|branch| !branch.is_remote)
            .count()
            <= 1
        {
            self.messages.error_message =
                "Cannot delete the only local branch in this repository.".to_string();
            cx.notify();
            return;
        }

        self.nav.active_dialog = ActiveDialog::DeleteBranch { branch_name };
        self.messages.error_message.clear();
        cx.notify();
    }

    pub fn menu_update_from_default_branch(&mut self, cx: &mut Context<Self>) {
        self.update_from_default_branch(cx);
    }

    pub fn menu_merge_branch(&mut self, cx: &mut Context<Self>) {
        if self.repo.snapshot.is_none() {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        }

        self.nav.show_branch_selector = true;
        self.nav.branch_selector_mode = BranchSelectorMode::Merge;
        self.repo.pending_cherry_pick_oid = None;
        self.messages.status_message =
            "Choose a branch to merge into the current branch.".to_string();
        cx.notify();
    }

    pub fn menu_rebase_branch(&mut self, cx: &mut Context<Self>) {
        if self.repo.snapshot.is_none() {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        }

        self.nav.show_branch_selector = true;
        self.nav.branch_selector_mode = BranchSelectorMode::Rebase;
        self.repo.pending_cherry_pick_oid = None;
        self.messages.status_message =
            "Choose a branch to rebase the current branch onto.".to_string();
        self.messages.error_message.clear();
        cx.notify();
    }

    pub fn menu_compare_branch(&mut self, cx: &mut Context<Self>) {
        if self.repo.snapshot.is_none() {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        }

        self.nav.sidebar_tab = SidebarTab::History;
        self.nav.show_branch_selector = true;
        self.nav.branch_selector_mode = BranchSelectorMode::Compare;
        self.repo.pending_cherry_pick_oid = None;
        self.messages.status_message =
            "Choose a branch to compare against the current branch.".to_string();
        self.messages.error_message.clear();
        cx.notify();
    }

    pub(crate) fn compare_branch(&mut self, target_branch: String, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        match self.git.compare_current_branch_with(&path, &target_branch) {
            Ok(comparison) => {
                self.messages.status_message = branch_comparison_message(&comparison);
                self.messages.error_message.clear();
                self.nav.sidebar_tab = SidebarTab::History;
                self.selection.selected_commit =
                    comparison.commits.first().map(|commit| commit.oid.clone());
                self.selection.selected_commit_file =
                    comparison.diffs.first().map(|diff| diff.path.clone());
                self.selection.commit_diffs = None;
                self.repo.merge_target = comparison.target_branch.clone();
                self.repo.comparison = Some(comparison);
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Could not compare with '{target_branch}': {err}");
            }
        }
        cx.notify();
    }

    pub(super) fn default_branch_name(&self) -> String {
        self.repo
            .identity
            .default_branch
            .as_deref()
            .or(self.settings.default_branch.as_deref())
            .unwrap_or("main")
            .to_string()
    }

    pub fn menu_view_current_branch_on_github(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        self.handle_branch_context_action(
            snapshot.repo.current_branch.clone(),
            BranchContextAction::ViewOnGitHub,
            cx,
        );
    }

    pub fn menu_compare_current_branch_on_github(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let branch_name = snapshot.repo.current_branch.clone();
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        match self.git.github_compare_branch_url(&path, &branch_name) {
            Ok(Some(url)) => match open_url(&url) {
                Ok(_) => {
                    self.messages.status_message =
                        format!("Opened compare for branch '{branch_name}' on GitHub.");
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message = format!(
                        "Failed to open compare for branch '{branch_name}' on GitHub: {err}"
                    );
                }
            },
            Ok(None) => {
                self.messages.error_message =
                    "This repository does not have a GitHub remote URL.".to_string();
            }
            Err(err) => {
                self.messages.error_message = format!("Could not build GitHub compare URL: {err}");
            }
        }
        cx.notify();
    }

    pub fn menu_discard_all_changes(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let paths: Vec<String> = snapshot
            .changes
            .iter()
            .map(|change| change.path.clone())
            .collect();
        if paths.is_empty() {
            self.messages.error_message = "There are no local changes to discard.".to_string();
            cx.notify();
            return;
        }

        self.nav.active_dialog = ActiveDialog::DiscardChanges { paths };
        self.messages.error_message.clear();
        cx.notify();
    }

    pub fn menu_stash_changes(&mut self, cx: &mut Context<Self>) {
        self.show_stash_changes_dialog(cx);
    }

    pub fn menu_zoom_in(&mut self, cx: &mut Context<Self>) {
        self.rem_size = (self.rem_size + ZOOM_STEP).min(ZOOM_MAX);
        let pct = ((self.rem_size / DEFAULT_REM_SIZE) * 100.0).round() as i32;
        self.messages.status_message = format!("Zoom: {pct}%");
        cx.notify();
    }

    pub fn menu_zoom_out(&mut self, cx: &mut Context<Self>) {
        self.rem_size = (self.rem_size - ZOOM_STEP).max(ZOOM_MIN);
        let pct = ((self.rem_size / DEFAULT_REM_SIZE) * 100.0).round() as i32;
        self.messages.status_message = format!("Zoom: {pct}%");
        cx.notify();
    }

    pub fn menu_zoom_reset(&mut self, cx: &mut Context<Self>) {
        self.rem_size = DEFAULT_REM_SIZE;
        self.messages.status_message = "Zoom: 100%".to_string();
        cx.notify();
    }

    fn handle_menu_open_repository(
        &mut self,
        _: &crate::MenuOpenRepository,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_repo_dialog(cx);
    }

    fn handle_menu_new_repository(
        &mut self,
        _: &crate::MenuNewRepository,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_create_repository_dialog(cx);
    }

    fn handle_menu_clone_repository(
        &mut self,
        _: &crate::MenuCloneRepository,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_clone_repository_dialog(cx);
    }

    fn handle_menu_show_settings(
        &mut self,
        _: &crate::MenuShowSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_global_settings_modal(None, cx);
        cx.notify();
    }

    fn handle_menu_show_changes(
        &mut self,
        _: &crate::MenuShowChanges,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav.sidebar_tab = SidebarTab::Changes;
        cx.notify();
    }

    fn handle_menu_show_history(
        &mut self,
        _: &crate::MenuShowHistory,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav.sidebar_tab = SidebarTab::History;
        cx.notify();
    }

    fn handle_menu_show_repository_list(
        &mut self,
        _: &crate::MenuShowRepositoryList,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_show_repository_list(cx);
    }

    fn handle_menu_show_branches_list(
        &mut self,
        _: &crate::MenuShowBranchesList,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_show_branches_list(cx);
    }

    fn handle_menu_go_to_summary(
        &mut self,
        _: &crate::MenuGoToSummary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_go_to_summary(cx);
        window.focus(&self.summary_focus);
    }

    fn handle_menu_show_stashed_changes(
        &mut self,
        _: &crate::MenuShowStashedChanges,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_show_stashed_changes(cx);
    }

    fn handle_menu_fetch(
        &mut self,
        _: &crate::MenuFetch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_fetch(cx);
    }

    fn handle_menu_pull(
        &mut self,
        _: &crate::MenuPull,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_pull(cx);
    }

    fn handle_menu_push(
        &mut self,
        _: &crate::MenuPush,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_push(cx);
    }

    fn handle_menu_publish_repository(
        &mut self,
        _: &crate::MenuPublishRepository,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_publish_repository(cx);
    }

    fn handle_menu_open_external_editor(
        &mut self,
        _: &crate::MenuOpenExternalEditor,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_open_external_editor(cx);
    }

    fn handle_menu_open_in_terminal(
        &mut self,
        _: &crate::MenuOpenInTerminal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_open_in_terminal(cx);
    }

    fn handle_menu_show_in_finder(
        &mut self,
        _: &crate::MenuShowInFinder,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_show_in_finder(cx);
    }

    fn handle_menu_view_on_github(
        &mut self,
        _: &crate::MenuViewOnGitHub,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_view_on_github(cx);
    }

    fn handle_menu_repository_settings(
        &mut self,
        _: &crate::MenuRepositorySettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_repository_settings(cx);
        if self.nav.show_settings {
            self.activate_settings_field(SettingsField::GitUserName, window, cx);
        }
    }

    fn handle_menu_new_branch(
        &mut self,
        _: &crate::MenuNewBranch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_new_branch(cx);
        if matches!(self.nav.active_dialog, ActiveDialog::CreateBranch) {
            window.focus(&self.new_branch_focus);
        }
    }

    fn handle_menu_rename_branch(
        &mut self,
        _: &crate::MenuRenameBranch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_rename_current_branch(cx);
    }

    fn handle_menu_delete_branch(
        &mut self,
        _: &crate::MenuDeleteBranch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_delete_current_branch(cx);
    }

    fn handle_menu_update_from_default_branch(
        &mut self,
        _: &crate::MenuUpdateFromDefaultBranch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_update_from_default_branch(cx);
    }

    fn handle_menu_compare_branch(
        &mut self,
        _: &crate::MenuCompareBranch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_compare_branch(cx);
    }

    fn handle_menu_merge_branch(
        &mut self,
        _: &crate::MenuMergeBranch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_merge_branch(cx);
    }

    fn handle_menu_rebase_branch(
        &mut self,
        _: &crate::MenuRebaseBranch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_rebase_branch(cx);
    }

    fn handle_menu_compare_on_github(
        &mut self,
        _: &crate::MenuCompareOnGitHub,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_compare_current_branch_on_github(cx);
    }

    fn handle_menu_view_branch_on_github(
        &mut self,
        _: &crate::MenuViewBranchOnGitHub,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_view_current_branch_on_github(cx);
    }

    fn handle_menu_discard_all_changes(
        &mut self,
        _: &crate::MenuDiscardAllChanges,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_discard_all_changes(cx);
    }

    fn handle_menu_stash_changes(
        &mut self,
        _: &crate::MenuStashChanges,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_stash_changes(cx);
    }

    fn handle_menu_zoom_in(
        &mut self,
        _: &crate::MenuZoomIn,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rem_size = (self.rem_size + ZOOM_STEP).min(ZOOM_MAX);
        let pct = ((self.rem_size / DEFAULT_REM_SIZE) * 100.0).round() as i32;
        self.messages.status_message = format!("Zoom: {pct}%");
        cx.notify();
    }

    fn handle_menu_zoom_out(
        &mut self,
        _: &crate::MenuZoomOut,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rem_size = (self.rem_size - ZOOM_STEP).max(ZOOM_MIN);
        let pct = ((self.rem_size / DEFAULT_REM_SIZE) * 100.0).round() as i32;
        self.messages.status_message = format!("Zoom: {pct}%");
        cx.notify();
    }

    fn handle_menu_zoom_reset(
        &mut self,
        _: &crate::MenuZoomReset,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rem_size = DEFAULT_REM_SIZE;
        self.messages.status_message = "Zoom: 100%".to_string();
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Toolbar
    // ------------------------------------------------------------------

    /// Returns (left_toolbar, right_toolbar) so they can go into separate resizable columns.
    fn render_toolbar_parts(&self, cx: &mut Context<Self>) -> (Div, Div) {
        use crate::ui::toolbar;

        let snapshot = self.repo.snapshot.as_ref();
        let repo_name = snapshot
            .map(|s| s.repo.name.as_str())
            .unwrap_or("Choose repository");
        let branch_name = snapshot
            .map(|s| s.repo.current_branch.as_str())
            .unwrap_or("No branch");
        let ahead = snapshot.map(|s| s.repo.ahead).unwrap_or(0);
        let behind = snapshot.map(|s| s.repo.behind).unwrap_or(0);

        let network_action = snapshot
            .map(|s| NetworkAction::from_snapshot(s))
            .unwrap_or(NetworkAction::Fetch);
        let remote_name = snapshot
            .and_then(|s| s.repo.remote_name.as_deref())
            .unwrap_or("origin");
        let is_in_flight = self.network.active_action.is_some();
        let network_label = if let Some(active) = self.network.active_action {
            active.pending_title(remote_name)
        } else {
            network_action.title(remote_name)
        };
        let last_fetched = snapshot.and_then(|s| s.repo.last_fetched.as_deref());
        let network_enabled = snapshot.is_some();

        // --- Left: repo section ---
        // Icon: lock for repos with remote (private-like), folder for local-only
        let repo_icon = if snapshot.and_then(|s| s.repo.remote_name.as_ref()).is_some() {
            toolbar::ToolbarIcon::Svg("icons/lock.svg")
        } else {
            toolbar::ToolbarIcon::Name(IconName::FolderClosed)
        };
        let repo_section = toolbar::render_toolbar_section(
            "section-repo",
            repo_icon,
            "Current Repository",
            repo_name,
            self.nav.show_repo_selector,
            false,
            false,
        )
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.handle_toolbar_action(ToolbarAction::ToggleRepoSelector, cx);
            if app.nav.show_repo_selector {
                _win.focus(&app.repo_filter_focus);
            }
        }));

        let left = h_flex()
            .w_full()
            .h(theme::z(theme::TOOLBAR_HEIGHT))
            .flex_shrink_0()
            .bg(theme::toolbar_bg())
            .border_b_1()
            .border_color(theme::toolbar_button_border())
            .child(repo_section);

        // --- Right: worktree + branch + network ---
        // The worktree name is the repo directory's own name, so the toolbar
        // can label it without shelling out; the LIST is loaded lazily when
        // the picker opens.
        let worktree_name = snapshot
            .map(|snapshot| snapshot.repo.name.clone())
            .unwrap_or_else(|| "\u{2014}".to_string());
        let worktree_section = toolbar::render_toolbar_section(
            "section-worktree",
            toolbar::ToolbarIcon::Name(IconName::FolderClosed),
            "Current Worktree",
            &worktree_name,
            self.nav.show_worktree_selector,
            false,
            snapshot.is_none(),
        )
        .flex_none()
        .w(px(toolbar::WORKTREE_SECTION_WIDTH))
        .on_click(cx.listener(|app, _evt, window, cx| {
            if app.repo.snapshot.is_none() {
                return;
            }
            app.toggle_worktree_selector(window, cx);
        }));

        let branch_section = toolbar::render_toolbar_section(
            "section-branch",
            toolbar::ToolbarIcon::Svg("icons/git-branch.svg"),
            "Current Branch",
            branch_name,
            self.nav.show_branch_selector,
            false,
            snapshot.is_none(),
        )
        .flex_none()
        .w(px(toolbar::BRANCH_SECTION_WIDTH))
        .on_click(cx.listener(|app, _evt, _win, cx| {
            if app.repo.snapshot.is_none() {
                return;
            }
            app.nav.show_branch_selector = !app.nav.show_branch_selector;
            if !app.nav.show_branch_selector {
                app.repo.pending_cherry_pick_oid = None;
            }
            app.nav.branch_selector_mode = BranchSelectorMode::Switch;
            app.nav.show_repo_selector = false;
            app.nav.show_worktree_selector = false;
            app.nav.show_network_dropdown = false;
            cx.notify();
        }));

        let has_network_dropdown =
            matches!(network_action, NetworkAction::Pull | NetworkAction::Push);
        let show_network_dropdown = self.nav.show_network_dropdown && has_network_dropdown;
        let (network_main, network_caret) = toolbar::render_network_parts(
            &network_label,
            ahead,
            behind,
            last_fetched,
            is_in_flight,
            show_network_dropdown,
            !network_enabled,
        );

        let net_action = network_action;
        let network_main =
            network_main
                .pr(theme::z(10.0))
                .on_click(cx.listener(move |app, _evt, _win, cx| {
                    if app.repo.snapshot.is_some() && app.network.active_action.is_none() {
                        app.nav.show_network_dropdown = false;
                        if net_action == NetworkAction::PublishRepository {
                            app.open_publish_dialog(_win, cx);
                        } else {
                            app.handle_toolbar_action(
                                ToolbarAction::RunNetworkAction(net_action),
                                cx,
                            );
                        }
                    }
                }));
        let network_caret = network_caret.on_click(cx.listener(|app, _evt, _win, cx| {
            if app.repo.snapshot.is_none() {
                return;
            }
            let Some(snapshot) = app.repo.snapshot.as_ref() else {
                return;
            };
            let action = NetworkAction::from_snapshot(snapshot);
            if !matches!(action, NetworkAction::Pull | NetworkAction::Push) {
                app.nav.show_network_dropdown = false;
                cx.notify();
                return;
            }
            app.nav.show_network_dropdown = !app.nav.show_network_dropdown;
            app.nav.show_repo_selector = false;
            app.nav.show_branch_selector = false;
            app.nav.branch_selector_mode = BranchSelectorMode::Switch;
            cx.notify();
        }));

        let right = h_flex()
            .w_full()
            .h(theme::z(theme::TOOLBAR_HEIGHT))
            .flex_shrink_0()
            .bg(theme::toolbar_bg())
            .border_b_1()
            .border_color(theme::toolbar_button_border())
            .child(worktree_section)
            .child(toolbar::vertical_divider())
            .child(branch_section)
            .child(toolbar::vertical_divider())
            .child(
                div()
                    .flex_none()
                    .w(px(toolbar::NETWORK_SECTION_WIDTH))
                    .h_full()
                    .child(h_flex().size_full().child(network_main).children(
                        if has_network_dropdown {
                            Some(network_caret)
                        } else {
                            None
                        },
                    )),
            );

        (left, right)
    }

    pub(crate) fn prepare_commit_summary_field_for_automation(&mut self) {
        self.summary_cursor = self.commit.summary.len();
        self.summary_selection = None;
    }

    pub(crate) fn prepare_commit_body_field_for_automation(&mut self) {
        self.description_cursor = self.commit.body.len();
        self.description_selection = None;
    }

    pub(crate) fn prepare_branch_filter_field_for_automation(&mut self) {
        self.branch_filter_cursor = self.filters.branch_filter_text.len();
    }

    pub(crate) fn prepare_repo_filter_field_for_automation(&mut self) {
        self.repo_filter_cursor = self.filters.repo_filter_text.len();
    }

    pub(crate) fn prepare_new_branch_field_for_automation(&mut self) {
        self.new_branch_cursor = self.repo.new_branch_name.len();
        self.new_branch_selection = None;
    }

    // ------------------------------------------------------------------
    // Sidebar
    // ------------------------------------------------------------------

    fn render_sidebar(
        &self,
        summary_focused: bool,
        description_focused: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity().clone();
        let sidebar_tab = self.nav.sidebar_tab;

        let mut sidebar = crate::ui::sidebar::render_sidebar_interactive(self, view, cx);

        // Commit form with interactive handlers (only on Changes tab)
        if sidebar_tab == SidebarTab::Changes {
            // Undo commit banner (auto-dismiss after 15 seconds)
            if let Some((summary, created_at)) = self
                .nav
                .undo_commit
                .as_ref()
                .filter(|_| self.can_undo_last_commit())
            {
                let elapsed = created_at.elapsed().as_secs();
                if elapsed < 15 {
                    let summary_text = if summary.len() > 30 {
                        format!("{}...", &summary[..27])
                    } else {
                        summary.clone()
                    };
                    sidebar = sidebar.child(
                        h_flex()
                            .w_full()
                            .h(theme::z(32.0))
                            .px(theme::z(10.0))
                            .items_center()
                            .gap(theme::z(6.0))
                            .bg(theme::surface_bg())
                            .border_t_1()
                            .border_color(theme::border())
                            .flex_shrink_0()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(theme::z(11.0))
                                    .text_color(theme::text_muted())
                                    .overflow_x_hidden()
                                    .whitespace_nowrap()
                                    .child(format!("\u{201C}{summary_text}\u{201D}")),
                            )
                            .child(
                                div()
                                    .id("undo-commit-btn")
                                    .px(theme::z(8.0))
                                    .py(theme::z(2.0))
                                    .rounded(theme::z(4.0))
                                    .bg(theme::accent())
                                    .text_size(theme::z(11.0))
                                    .text_color(theme::on_accent())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::commit_button_hover_bg()))
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.undo_last_commit(cx);
                                    }))
                                    .child("Undo"),
                            ),
                    );
                } else {
                    // Auto-dismiss
                    // (Can't mutate here in render, but it'll clear on next event)
                }
            }

            let branch_name = self
                .repo
                .snapshot
                .as_ref()
                .map(|s| s.repo.current_branch.clone())
                .unwrap_or_else(|| "main".to_string());
            sidebar = sidebar.child(self.render_commit_form_interactive(
                &branch_name,
                summary_focused,
                description_focused,
                cx,
            ));
        }

        sidebar
    }

    // ------------------------------------------------------------------
    // Text input key handling
    // ------------------------------------------------------------------

    fn handle_summary_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_summary_key_for_automation(event, cx);
    }

    pub(crate) fn apply_summary_key_for_automation(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            match ks.key.as_str() {
                "v" => {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            let text = text.replace('\n', " ");
                            self.delete_summary_selection();
                            self.commit.summary.insert_str(self.summary_cursor, &text);
                            self.summary_cursor += text.len();
                            cx.notify();
                        }
                    }
                }
                "a" => {
                    // Select all
                    self.summary_selection = Some(0);
                    self.summary_cursor = self.commit.summary.len();
                    cx.notify();
                }
                "c" => {
                    // Copy selection
                    if let Some(sel) = self.summary_selection {
                        let (start, end) = ordered_range(sel, self.summary_cursor);
                        let selected = &self.commit.summary[start..end];
                        if !selected.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                        }
                    }
                }
                "x" => {
                    // Cut selection
                    if let Some(sel) = self.summary_selection {
                        let (start, end) = ordered_range(sel, self.summary_cursor);
                        let selected = &self.commit.summary[start..end];
                        if !selected.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                            self.delete_summary_selection();
                            cx.notify();
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.summary_selection.is_some() {
                    self.delete_summary_selection();
                } else if self.summary_cursor > 0 {
                    let new_pos = prev_char_boundary(&self.commit.summary, self.summary_cursor);
                    self.commit.summary.drain(new_pos..self.summary_cursor);
                    self.summary_cursor = new_pos;
                }
                cx.notify();
            }
            "delete" => {
                if self.summary_selection.is_some() {
                    self.delete_summary_selection();
                } else if self.summary_cursor < self.commit.summary.len() {
                    let end = next_char_boundary(&self.commit.summary, self.summary_cursor);
                    self.commit.summary.drain(self.summary_cursor..end);
                }
                cx.notify();
            }
            "left" => {
                if ks.modifiers.shift {
                    if self.summary_selection.is_none() {
                        self.summary_selection = Some(self.summary_cursor);
                    }
                } else {
                    self.summary_selection = None;
                }
                if self.summary_cursor > 0 {
                    self.summary_cursor =
                        prev_char_boundary(&self.commit.summary, self.summary_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if ks.modifiers.shift {
                    if self.summary_selection.is_none() {
                        self.summary_selection = Some(self.summary_cursor);
                    }
                } else {
                    self.summary_selection = None;
                }
                if self.summary_cursor < self.commit.summary.len() {
                    self.summary_cursor =
                        next_char_boundary(&self.commit.summary, self.summary_cursor);
                    cx.notify();
                }
            }
            "home" => {
                if ks.modifiers.shift {
                    if self.summary_selection.is_none() {
                        self.summary_selection = Some(self.summary_cursor);
                    }
                } else {
                    self.summary_selection = None;
                }
                self.summary_cursor = 0;
                cx.notify();
            }
            "end" => {
                if ks.modifiers.shift {
                    if self.summary_selection.is_none() {
                        self.summary_selection = Some(self.summary_cursor);
                    }
                } else {
                    self.summary_selection = None;
                }
                self.summary_cursor = self.commit.summary.len();
                cx.notify();
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control && !ch.contains('\n') && !ch.contains('\r') {
                        self.delete_summary_selection();
                        self.commit.summary.insert_str(self.summary_cursor, ch);
                        self.summary_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    fn delete_summary_selection(&mut self) {
        if let Some(sel) = self.summary_selection.take() {
            let (start, end) = ordered_range(sel, self.summary_cursor);
            if start < end && end <= self.commit.summary.len() {
                self.commit.summary.drain(start..end);
                self.summary_cursor = start;
            }
        }
    }

    fn handle_description_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_description_key_for_automation(event, cx);
    }

    pub(crate) fn apply_description_key_for_automation(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            match ks.key.as_str() {
                "v" => {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            self.delete_description_selection();
                            self.commit.body.insert_str(self.description_cursor, &text);
                            self.description_cursor += text.len();
                            cx.notify();
                        }
                    }
                }
                "a" => {
                    self.description_selection = Some(0);
                    self.description_cursor = self.commit.body.len();
                    cx.notify();
                }
                "c" => {
                    if let Some(sel) = self.description_selection {
                        let (start, end) = ordered_range(sel, self.description_cursor);
                        let selected = &self.commit.body[start..end];
                        if !selected.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                        }
                    }
                }
                "x" => {
                    if let Some(sel) = self.description_selection {
                        let (start, end) = ordered_range(sel, self.description_cursor);
                        let selected = &self.commit.body[start..end];
                        if !selected.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                            self.delete_description_selection();
                            cx.notify();
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.description_selection.is_some() {
                    self.delete_description_selection();
                } else if self.description_cursor > 0 {
                    let new_pos = prev_char_boundary(&self.commit.body, self.description_cursor);
                    self.commit.body.drain(new_pos..self.description_cursor);
                    self.description_cursor = new_pos;
                }
                cx.notify();
            }
            "delete" => {
                if self.description_selection.is_some() {
                    self.delete_description_selection();
                } else if self.description_cursor < self.commit.body.len() {
                    let end = next_char_boundary(&self.commit.body, self.description_cursor);
                    self.commit.body.drain(self.description_cursor..end);
                }
                cx.notify();
            }
            "left" => {
                if ks.modifiers.shift {
                    if self.description_selection.is_none() {
                        self.description_selection = Some(self.description_cursor);
                    }
                } else {
                    self.description_selection = None;
                }
                if self.description_cursor > 0 {
                    self.description_cursor =
                        prev_char_boundary(&self.commit.body, self.description_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if ks.modifiers.shift {
                    if self.description_selection.is_none() {
                        self.description_selection = Some(self.description_cursor);
                    }
                } else {
                    self.description_selection = None;
                }
                if self.description_cursor < self.commit.body.len() {
                    self.description_cursor =
                        next_char_boundary(&self.commit.body, self.description_cursor);
                    cx.notify();
                }
            }
            "home" => {
                if ks.modifiers.shift {
                    if self.description_selection.is_none() {
                        self.description_selection = Some(self.description_cursor);
                    }
                } else {
                    self.description_selection = None;
                }
                self.description_cursor = 0;
                cx.notify();
            }
            "end" => {
                if ks.modifiers.shift {
                    if self.description_selection.is_none() {
                        self.description_selection = Some(self.description_cursor);
                    }
                } else {
                    self.description_selection = None;
                }
                self.description_cursor = self.commit.body.len();
                cx.notify();
            }
            "enter" => {
                self.delete_description_selection();
                self.commit.body.insert_str(self.description_cursor, "\n");
                self.description_cursor += 1;
                cx.notify();
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control {
                        self.delete_description_selection();
                        self.commit.body.insert_str(self.description_cursor, ch);
                        self.description_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    fn delete_description_selection(&mut self) {
        if let Some(sel) = self.description_selection.take() {
            let (start, end) = ordered_range(sel, self.description_cursor);
            if start < end && end <= self.commit.body.len() {
                self.commit.body.drain(start..end);
                self.description_cursor = start;
            }
        }
    }

    // ------------------------------------------------------------------
    // Filter input key handling
    // ------------------------------------------------------------------

    pub(super) fn handle_branch_filter_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_branch_filter_key_for_automation(event, cx);
    }

    pub(crate) fn apply_branch_filter_key_for_automation(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            if ks.key.as_str() == "v" {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        let text = text.replace('\n', "");
                        self.filters
                            .branch_filter_text
                            .insert_str(self.branch_filter_cursor, &text);
                        self.branch_filter_cursor += text.len();
                        cx.notify();
                    }
                }
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.branch_filter_cursor > 0 {
                    let new_pos = prev_char_boundary(
                        &self.filters.branch_filter_text,
                        self.branch_filter_cursor,
                    );
                    self.filters
                        .branch_filter_text
                        .drain(new_pos..self.branch_filter_cursor);
                    self.branch_filter_cursor = new_pos;
                    cx.notify();
                }
            }
            "escape" => {
                self.nav.show_branch_selector = false;
                self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                self.repo.pending_cherry_pick_oid = None;
                self.filters.branch_filter_text.clear();
                self.branch_filter_cursor = 0;
                cx.notify();
            }
            "left" => {
                if self.branch_filter_cursor > 0 {
                    self.branch_filter_cursor = prev_char_boundary(
                        &self.filters.branch_filter_text,
                        self.branch_filter_cursor,
                    );
                    cx.notify();
                }
            }
            "right" => {
                if self.branch_filter_cursor < self.filters.branch_filter_text.len() {
                    self.branch_filter_cursor = next_char_boundary(
                        &self.filters.branch_filter_text,
                        self.branch_filter_cursor,
                    );
                    cx.notify();
                }
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control && !ch.contains('\n') && !ch.contains('\r') {
                        self.filters
                            .branch_filter_text
                            .insert_str(self.branch_filter_cursor, ch);
                        self.branch_filter_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    pub(super) fn handle_new_branch_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_new_branch_key_for_automation(event, cx);
    }

    pub(crate) fn apply_new_branch_key_for_automation(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "enter" {
            match self.nav.active_dialog.clone() {
                ActiveDialog::CreateBranch if self.can_create_branch_from_dialog() => {
                    self.create_branch(cx);
                }
                ActiveDialog::CreateTag { target_oid }
                    if self.create_tag_validation_message().is_none() =>
                {
                    self.create_tag(target_oid, cx);
                }
                _ => {
                    cx.notify();
                }
            }
            return;
        }

        let mut state = crate::ui::text_field::TextFieldState {
            cursor: self.new_branch_cursor,
            selection: self.new_branch_selection,
        };
        let handled = crate::ui::text_field::handle_text_key(
            &mut self.repo.new_branch_name,
            &mut state,
            false,
            event,
            cx,
        );
        self.new_branch_cursor = state.cursor;
        self.new_branch_selection = state.selection;
        if handled {
            cx.notify();
        }
    }

    pub(crate) fn handle_publish_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.key == "escape" {
            self.nav.active_dialog = ActiveDialog::None;
            cx.notify();
            return;
        }
        if ks.key == "tab" {
            self.publish_active_field = Some(match self.publish_active_field {
                Some(PublishField::Name) => PublishField::Description,
                _ => PublishField::Name,
            });
            cx.notify();
            return;
        }

        let Some(field) = self.publish_active_field else {
            return;
        };

        let (value, cursor, selection) = match field {
            PublishField::Name => (
                &mut self.network.publish_name,
                &mut self.publish_name_cursor,
                &mut self.publish_name_selection,
            ),
            PublishField::Description => (
                &mut self.network.publish_description,
                &mut self.publish_description_cursor,
                &mut self.publish_description_selection,
            ),
        };
        let mut state = crate::ui::text_field::TextFieldState {
            cursor: *cursor,
            selection: *selection,
        };
        let handled = crate::ui::text_field::handle_text_key(value, &mut state, false, event, cx);
        *cursor = state.cursor;
        *selection = state.selection;
        if handled {
            cx.notify();
        }
    }

    pub(crate) fn repository_field_value(&self, field: RepositoryField) -> &str {
        match field {
            RepositoryField::CreateName => self.repo.create_repo_name.as_str(),
            RepositoryField::CreateDescription => self.repo.create_repo_description.as_str(),
            RepositoryField::CreatePath => self.repo.create_repo_path.as_str(),
            RepositoryField::CreateBranchName => self.repo.create_repo_branch_name.as_str(),
            RepositoryField::CloneUrl => self.repo.clone_repo_url.as_str(),
            RepositoryField::CloneName => self.repo.clone_repo_name.as_str(),
            RepositoryField::ClonePath => self.repo.clone_repo_path.as_str(),
        }
    }

    pub(crate) fn repository_field_cursor(&self, field: RepositoryField) -> usize {
        match field {
            RepositoryField::CreateName => self.repository_create_name_cursor,
            RepositoryField::CreateDescription => self.repository_create_description_cursor,
            RepositoryField::CreatePath => self.repository_create_path_cursor,
            RepositoryField::CreateBranchName => self.repository_create_branch_cursor,
            RepositoryField::CloneUrl => self.repository_clone_url_cursor,
            RepositoryField::CloneName => self.repository_clone_name_cursor,
            RepositoryField::ClonePath => self.repository_clone_path_cursor,
        }
    }

    pub(crate) fn repository_field_selection(&self, field: RepositoryField) -> Option<usize> {
        match field {
            RepositoryField::CreateName => self.repository_create_name_selection,
            RepositoryField::CreateDescription => self.repository_create_description_selection,
            RepositoryField::CreatePath => self.repository_create_path_selection,
            RepositoryField::CreateBranchName => self.repository_create_branch_selection,
            RepositoryField::CloneUrl => self.repository_clone_url_selection,
            RepositoryField::CloneName => self.repository_clone_name_selection,
            RepositoryField::ClonePath => self.repository_clone_path_selection,
        }
    }

    pub(crate) fn repository_field_focused(&self, field: RepositoryField, window: &Window) -> bool {
        self.repository_focus.is_focused(window) && self.repository_active_field == Some(field)
    }

    pub(crate) fn activate_repository_field(
        &mut self,
        field: RepositoryField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_repository_field_for_automation(field);
        window.focus(&self.repository_focus);
        cx.notify();
    }

    pub(crate) fn activate_repository_field_for_automation(&mut self, field: RepositoryField) {
        self.repository_active_field = Some(field);
        let cursor = self.repository_field_value(field).len();
        self.set_repository_field_cursor(field, cursor);
        self.set_repository_field_selection(field, None);
    }

    fn set_repository_field_cursor(&mut self, field: RepositoryField, cursor: usize) {
        match field {
            RepositoryField::CreateName => self.repository_create_name_cursor = cursor,
            RepositoryField::CreateDescription => {
                self.repository_create_description_cursor = cursor
            }
            RepositoryField::CreatePath => self.repository_create_path_cursor = cursor,
            RepositoryField::CreateBranchName => self.repository_create_branch_cursor = cursor,
            RepositoryField::CloneUrl => self.repository_clone_url_cursor = cursor,
            RepositoryField::CloneName => self.repository_clone_name_cursor = cursor,
            RepositoryField::ClonePath => self.repository_clone_path_cursor = cursor,
        }
    }

    fn set_repository_field_selection(&mut self, field: RepositoryField, selection: Option<usize>) {
        match field {
            RepositoryField::CreateName => self.repository_create_name_selection = selection,
            RepositoryField::CreateDescription => {
                self.repository_create_description_selection = selection
            }
            RepositoryField::CreatePath => self.repository_create_path_selection = selection,
            RepositoryField::CreateBranchName => {
                self.repository_create_branch_selection = selection
            }
            RepositoryField::CloneUrl => self.repository_clone_url_selection = selection,
            RepositoryField::CloneName => self.repository_clone_name_selection = selection,
            RepositoryField::ClonePath => self.repository_clone_path_selection = selection,
        }
    }

    pub(crate) fn handle_repository_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_repository_key_for_automation(event, cx);
    }

    pub(crate) fn apply_repository_key_for_automation(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" {
            self.nav.active_dialog = ActiveDialog::None;
            cx.notify();
            return;
        }
        if event.keystroke.key == "tab" {
            let next_field = match self.nav.active_dialog {
                ActiveDialog::CreateRepository => match self.repository_active_field {
                    Some(RepositoryField::CreateName) => RepositoryField::CreateDescription,
                    Some(RepositoryField::CreateDescription) => RepositoryField::CreatePath,
                    Some(RepositoryField::CreatePath) => RepositoryField::CreateBranchName,
                    _ => RepositoryField::CreateName,
                },
                ActiveDialog::CloneRepository => match self.repository_active_field {
                    Some(RepositoryField::CloneUrl) => RepositoryField::CloneName,
                    Some(RepositoryField::CloneName) => RepositoryField::ClonePath,
                    _ => RepositoryField::CloneUrl,
                },
                _ => return,
            };
            self.repository_active_field = Some(next_field);
            self.set_repository_field_cursor(
                next_field,
                self.repository_field_value(next_field).len(),
            );
            self.set_repository_field_selection(next_field, None);
            cx.notify();
            return;
        }

        let Some(field) = self.repository_active_field else {
            return;
        };

        let handled = match field {
            RepositoryField::CreateName => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_create_name_cursor,
                    selection: self.repository_create_name_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.create_repo_name,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_create_name_cursor = state.cursor;
                self.repository_create_name_selection = state.selection;
                handled
            }
            RepositoryField::CreateDescription => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_create_description_cursor,
                    selection: self.repository_create_description_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.create_repo_description,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_create_description_cursor = state.cursor;
                self.repository_create_description_selection = state.selection;
                handled
            }
            RepositoryField::CreatePath => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_create_path_cursor,
                    selection: self.repository_create_path_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.create_repo_path,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_create_path_cursor = state.cursor;
                self.repository_create_path_selection = state.selection;
                handled
            }
            RepositoryField::CreateBranchName => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_create_branch_cursor,
                    selection: self.repository_create_branch_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.create_repo_branch_name,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_create_branch_cursor = state.cursor;
                self.repository_create_branch_selection = state.selection;
                handled
            }
            RepositoryField::CloneUrl => {
                let previous_inferred_name =
                    inferred_clone_directory_name(&self.repo.clone_repo_url);
                let should_update_inferred_name = self.repo.clone_repo_name.trim().is_empty()
                    || self.repo.clone_repo_name == previous_inferred_name;
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_clone_url_cursor,
                    selection: self.repository_clone_url_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.clone_repo_url,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_clone_url_cursor = state.cursor;
                self.repository_clone_url_selection = state.selection;
                if handled && should_update_inferred_name {
                    self.repo.clone_repo_name =
                        inferred_clone_directory_name(&self.repo.clone_repo_url);
                    self.repository_clone_name_cursor = self.repo.clone_repo_name.len();
                    self.repository_clone_name_selection = None;
                }
                handled
            }
            RepositoryField::CloneName => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_clone_name_cursor,
                    selection: self.repository_clone_name_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.clone_repo_name,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_clone_name_cursor = state.cursor;
                self.repository_clone_name_selection = state.selection;
                handled
            }
            RepositoryField::ClonePath => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_clone_path_cursor,
                    selection: self.repository_clone_path_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.clone_repo_path,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_clone_path_cursor = state.cursor;
                self.repository_clone_path_selection = state.selection;
                handled
            }
        };
        if handled {
            cx.notify();
        }
    }

    #[allow(dead_code)]
    pub(super) fn handle_worktree_filter_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            if ks.key.as_str() == "v" {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let text = text.replace('\n', "");
                    self.filters
                        .worktree_filter_text
                        .insert_str(self.worktree_filter_cursor, &text);
                    self.worktree_filter_cursor += text.len();
                    cx.notify();
                }
            }
            return;
        }
        match ks.key.as_str() {
            "escape" => {
                self.nav.show_worktree_selector = false;
                cx.notify();
            }
            "backspace" => {
                if self.worktree_filter_cursor > 0 {
                    let start = prev_char_boundary(
                        &self.filters.worktree_filter_text,
                        self.worktree_filter_cursor,
                    );
                    self.filters
                        .worktree_filter_text
                        .drain(start..self.worktree_filter_cursor);
                    self.worktree_filter_cursor = start;
                    cx.notify();
                }
            }
            "left" => {
                self.worktree_filter_cursor = prev_char_boundary(
                    &self.filters.worktree_filter_text,
                    self.worktree_filter_cursor,
                );
                cx.notify();
            }
            "right" => {
                self.worktree_filter_cursor = next_char_boundary(
                    &self.filters.worktree_filter_text,
                    self.worktree_filter_cursor,
                );
                cx.notify();
            }
            _ => {
                if let Some(ch) = ks.key_char.as_ref() {
                    if !ch.is_empty() && !ch.chars().any(char::is_control) {
                        self.filters
                            .worktree_filter_text
                            .insert_str(self.worktree_filter_cursor, ch);
                        self.worktree_filter_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    pub(super) fn handle_repo_filter_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_repo_filter_key_for_automation(event, cx);
    }

    pub(crate) fn apply_repo_filter_key_for_automation(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            if ks.key.as_str() == "v" {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        let text = text.replace('\n', "");
                        self.filters
                            .repo_filter_text
                            .insert_str(self.repo_filter_cursor, &text);
                        self.repo_filter_cursor += text.len();
                        cx.notify();
                    }
                }
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.repo_filter_cursor > 0 {
                    let new_pos =
                        prev_char_boundary(&self.filters.repo_filter_text, self.repo_filter_cursor);
                    self.filters
                        .repo_filter_text
                        .drain(new_pos..self.repo_filter_cursor);
                    self.repo_filter_cursor = new_pos;
                    cx.notify();
                }
            }
            "escape" => {
                self.nav.show_repo_selector = false;
                self.filters.repo_filter_text.clear();
                self.repo_filter_cursor = 0;
                cx.notify();
            }
            "left" => {
                if self.repo_filter_cursor > 0 {
                    self.repo_filter_cursor =
                        prev_char_boundary(&self.filters.repo_filter_text, self.repo_filter_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if self.repo_filter_cursor < self.filters.repo_filter_text.len() {
                    self.repo_filter_cursor =
                        next_char_boundary(&self.filters.repo_filter_text, self.repo_filter_cursor);
                    cx.notify();
                }
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control && !ch.contains('\n') && !ch.contains('\r') {
                        self.filters
                            .repo_filter_text
                            .insert_str(self.repo_filter_cursor, ch);
                        self.repo_filter_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Settings modal input handling
    // ------------------------------------------------------------------

    pub(crate) fn close_settings_modal(&mut self) {
        self.nav.show_settings = false;
        self.settings_modal.active_field = None;
        self.close_history_context_menu();
    }

    pub(crate) fn open_settings_modal(
        &mut self,
        section: Option<crate::ui::ui_state::SettingsSection>,
        cx: &mut Context<Self>,
    ) {
        self.open_global_settings_modal(section, cx);
    }

    pub(crate) fn open_global_settings_modal(
        &mut self,
        section: Option<crate::ui::ui_state::SettingsSection>,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_modal_with_scope(section, SettingsScope::Global, cx);
    }

    pub(crate) fn open_repository_settings_modal(
        &mut self,
        section: Option<crate::ui::ui_state::SettingsSection>,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_modal_with_scope(section, SettingsScope::Repository, cx);
    }

    fn open_settings_modal_with_scope(
        &mut self,
        section: Option<crate::ui::ui_state::SettingsSection>,
        scope: SettingsScope,
        cx: &mut Context<Self>,
    ) {
        let scope = if scope == SettingsScope::Repository && self.repo.snapshot.is_none() {
            SettingsScope::Global
        } else {
            scope
        };
        self.nav.settings_section =
            scope.normalize_section(section.unwrap_or(self.nav.settings_section));

        self.close_history_context_menu();
        self.nav.settings_scope = scope;
        self.nav.show_settings = true;
        if self.nav.settings_section == crate::ui::ui_state::SettingsSection::Remote
            && let Some(path) = self.repo_path().map(PathBuf::from)
        {
            self.load_remote_settings(&path);
        } else if self.nav.settings_section == crate::ui::ui_state::SettingsSection::IgnoredFiles
            && let Some(path) = self.repo_path().map(PathBuf::from)
        {
            self.load_ignored_files_settings(&path);
        } else if self.nav.settings_section == crate::ui::ui_state::SettingsSection::Git {
            if self.settings_has_repository_scope()
                && let Some(path) = self.repo_path().map(PathBuf::from)
            {
                self.load_identity(&path);
            } else {
                self.load_global_identity();
            }
        }
        let field = if self.nav.settings_section == crate::ui::ui_state::SettingsSection::Ai
            && self.settings.ai.provider == AiProvider::OpenRouter
        {
            SettingsField::OpenRouterModelFilter
        } else {
            settings_modal::default_settings_field(self.nav.settings_section)
        };
        self.settings_modal.active_field = Some(field);
        self.set_settings_field_cursor(field, self.settings_field_value(field).len());

        if self.nav.settings_section == crate::ui::ui_state::SettingsSection::Ai
            && self.settings.ai.provider == AiProvider::OpenRouter
        {
            self.ensure_openrouter_models(cx);
        }
    }

    pub(crate) fn settings_field_value(&self, field: SettingsField) -> &str {
        match field {
            SettingsField::RemoteUrl => self.repo.remote_url.as_str(),
            SettingsField::IgnoredFiles => self.repo.ignored_files_text.as_str(),
            SettingsField::GitUserName => self.active_git_settings_identity().user_name.as_str(),
            SettingsField::GitUserEmail => self.active_git_settings_identity().user_email.as_str(),
            SettingsField::GitDefaultBranch => self
                .active_git_settings_identity()
                .default_branch
                .as_deref()
                .unwrap_or(""),
            SettingsField::AiModel => self.settings.ai.model.as_str(),
            SettingsField::AiEndpoint => self.settings.ai.endpoint.as_str(),
            SettingsField::AiApiKey => self.settings.ai.api_key.as_str(),
            SettingsField::AiSystemPrompt => self.settings.ai.system_prompt.as_str(),
            SettingsField::OpenRouterModelFilter => self.filters.openrouter_model_filter.as_str(),
        }
    }

    pub(crate) fn active_git_settings_identity(&self) -> &GitIdentity {
        if self.settings_has_repository_scope() {
            if self.repo.use_local_identity {
                &self.repo.local_identity
            } else {
                &self.repo.global_identity
            }
        } else {
            &self.repo.global_identity
        }
    }

    pub(crate) fn active_git_settings_identity_mut(&mut self) -> &mut GitIdentity {
        if self.settings_has_repository_scope() {
            if self.repo.use_local_identity {
                &mut self.repo.local_identity
            } else {
                &mut self.repo.global_identity
            }
        } else {
            &mut self.repo.global_identity
        }
    }

    pub(crate) fn settings_field_read_only(&self, field: SettingsField) -> bool {
        self.settings_has_repository_scope()
            && !self.repo.use_local_identity
            && matches!(
                field,
                SettingsField::GitUserName | SettingsField::GitUserEmail
            )
    }

    pub(crate) fn settings_has_repository_scope(&self) -> bool {
        self.nav.settings_scope == SettingsScope::Repository && self.repo.snapshot.is_some()
    }

    pub(crate) fn settings_field_cursor(&self, field: SettingsField) -> usize {
        match field {
            SettingsField::RemoteUrl => self.settings_modal.remote_url_cursor,
            SettingsField::IgnoredFiles => self.settings_modal.ignored_files_cursor,
            SettingsField::GitUserName => self.settings_modal.git_user_name_cursor,
            SettingsField::GitUserEmail => self.settings_modal.git_user_email_cursor,
            SettingsField::GitDefaultBranch => self.settings_modal.git_default_branch_cursor,
            SettingsField::AiModel => self.settings_modal.ai_model_cursor,
            SettingsField::AiEndpoint => self.settings_modal.ai_endpoint_cursor,
            SettingsField::AiApiKey => self.settings_modal.ai_api_key_cursor,
            SettingsField::AiSystemPrompt => self.settings_modal.ai_system_prompt_cursor,
            SettingsField::OpenRouterModelFilter => {
                self.settings_modal.openrouter_model_filter_cursor
            }
        }
    }

    pub(crate) fn settings_field_selection(&self, field: SettingsField) -> Option<usize> {
        match field {
            SettingsField::RemoteUrl => self.settings_modal.remote_url_selection,
            SettingsField::IgnoredFiles => self.settings_modal.ignored_files_selection,
            SettingsField::GitUserName => self.settings_modal.git_user_name_selection,
            SettingsField::GitUserEmail => self.settings_modal.git_user_email_selection,
            SettingsField::GitDefaultBranch => self.settings_modal.git_default_branch_selection,
            SettingsField::AiModel => self.settings_modal.ai_model_selection,
            SettingsField::AiEndpoint => self.settings_modal.ai_endpoint_selection,
            SettingsField::AiApiKey => self.settings_modal.ai_api_key_selection,
            SettingsField::AiSystemPrompt => self.settings_modal.ai_system_prompt_selection,
            SettingsField::OpenRouterModelFilter => {
                self.settings_modal.openrouter_model_filter_selection
            }
        }
    }

    pub(crate) fn settings_field_focused(&self, field: SettingsField, window: &Window) -> bool {
        self.settings_modal.focus.is_focused(window)
            && self.settings_modal.active_field == Some(field)
    }

    pub(crate) fn activate_settings_field(
        &mut self,
        field: SettingsField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_settings_field_for_automation(field);
        window.focus(&self.settings_modal.focus);
        cx.notify();
    }

    pub(crate) fn activate_settings_field_for_automation(&mut self, field: SettingsField) {
        let cursor = self.settings_field_value(field).len();
        self.settings_modal.active_field = Some(field);
        self.set_settings_field_cursor(field, cursor);
    }

    pub(crate) fn handle_settings_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_settings_key_for_automation(event, cx);
    }

    pub(crate) fn apply_settings_key_for_automation(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" {
            self.close_settings_modal();
            cx.notify();
            return;
        }

        let Some(field) = self.settings_modal.active_field else {
            return;
        };
        if self.settings_field_read_only(field) {
            return;
        }

        let multiline = matches!(
            field,
            SettingsField::IgnoredFiles | SettingsField::AiSystemPrompt
        );

        // Get mutable references to the value, cursor, and selection for the active field
        let handled = match field {
            SettingsField::RemoteUrl => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.remote_url_cursor,
                    selection: self.settings_modal.remote_url_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.repo.remote_url,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.remote_url_cursor = state.cursor;
                self.settings_modal.remote_url_selection = state.selection;
                h
            }
            SettingsField::IgnoredFiles => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.ignored_files_cursor,
                    selection: self.settings_modal.ignored_files_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.repo.ignored_files_text,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.ignored_files_cursor = state.cursor;
                self.settings_modal.ignored_files_selection = state.selection;
                h
            }
            SettingsField::GitUserName => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.git_user_name_cursor,
                    selection: self.settings_modal.git_user_name_selection,
                };
                let mut value = self.active_git_settings_identity().user_name.clone();
                let h = crate::ui::text_field::handle_text_key(
                    &mut value, &mut state, multiline, event, cx,
                );
                self.active_git_settings_identity_mut().user_name = value;
                self.settings_modal.git_user_name_cursor = state.cursor;
                self.settings_modal.git_user_name_selection = state.selection;
                h
            }
            SettingsField::GitUserEmail => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.git_user_email_cursor,
                    selection: self.settings_modal.git_user_email_selection,
                };
                let mut value = self.active_git_settings_identity().user_email.clone();
                let h = crate::ui::text_field::handle_text_key(
                    &mut value, &mut state, multiline, event, cx,
                );
                self.active_git_settings_identity_mut().user_email = value;
                self.settings_modal.git_user_email_cursor = state.cursor;
                self.settings_modal.git_user_email_selection = state.selection;
                h
            }
            SettingsField::GitDefaultBranch => {
                let mut value = self
                    .active_git_settings_identity()
                    .default_branch
                    .clone()
                    .unwrap_or_default();
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.git_default_branch_cursor,
                    selection: self.settings_modal.git_default_branch_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut value, &mut state, multiline, event, cx,
                );
                self.active_git_settings_identity_mut().default_branch = if value.trim().is_empty()
                {
                    None
                } else {
                    Some(value)
                };
                self.settings_modal.git_default_branch_cursor = state.cursor;
                self.settings_modal.git_default_branch_selection = state.selection;
                h
            }
            SettingsField::AiModel => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.ai_model_cursor,
                    selection: self.settings_modal.ai_model_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.settings.ai.model,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.ai_model_cursor = state.cursor;
                self.settings_modal.ai_model_selection = state.selection;
                h
            }
            SettingsField::AiEndpoint => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.ai_endpoint_cursor,
                    selection: self.settings_modal.ai_endpoint_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.settings.ai.endpoint,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.ai_endpoint_cursor = state.cursor;
                self.settings_modal.ai_endpoint_selection = state.selection;
                h
            }
            SettingsField::AiApiKey => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.ai_api_key_cursor,
                    selection: self.settings_modal.ai_api_key_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.settings.ai.api_key,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.ai_api_key_cursor = state.cursor;
                self.settings_modal.ai_api_key_selection = state.selection;
                h
            }
            SettingsField::AiSystemPrompt => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.ai_system_prompt_cursor,
                    selection: self.settings_modal.ai_system_prompt_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.settings.ai.system_prompt,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.ai_system_prompt_cursor = state.cursor;
                self.settings_modal.ai_system_prompt_selection = state.selection;
                h
            }
            SettingsField::OpenRouterModelFilter => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.openrouter_model_filter_cursor,
                    selection: self.settings_modal.openrouter_model_filter_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.filters.openrouter_model_filter,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.openrouter_model_filter_cursor = state.cursor;
                self.settings_modal.openrouter_model_filter_selection = state.selection;
                h
            }
        };
        if handled {
            cx.notify();
        }
    }

    pub(super) fn set_settings_field_cursor(&mut self, field: SettingsField, cursor: usize) {
        match field {
            SettingsField::RemoteUrl => self.settings_modal.remote_url_cursor = cursor,
            SettingsField::IgnoredFiles => self.settings_modal.ignored_files_cursor = cursor,
            SettingsField::GitUserName => self.settings_modal.git_user_name_cursor = cursor,
            SettingsField::GitUserEmail => self.settings_modal.git_user_email_cursor = cursor,
            SettingsField::GitDefaultBranch => {
                self.settings_modal.git_default_branch_cursor = cursor
            }
            SettingsField::AiModel => self.settings_modal.ai_model_cursor = cursor,
            SettingsField::AiEndpoint => self.settings_modal.ai_endpoint_cursor = cursor,
            SettingsField::AiApiKey => self.settings_modal.ai_api_key_cursor = cursor,
            SettingsField::AiSystemPrompt => self.settings_modal.ai_system_prompt_cursor = cursor,
            SettingsField::OpenRouterModelFilter => {
                self.settings_modal.openrouter_model_filter_cursor = cursor
            }
        }
    }

    // ------------------------------------------------------------------
    // Text field renderer
    // ------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn render_text_field(
        &self,
        id: &str,
        value: &str,
        placeholder: &str,
        cursor: usize,
        selection: Option<usize>,
        focused: bool,
        multiline: bool,
        focus_handle: &FocusHandle,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let focused = focused && enabled;
        let is_empty = value.trim().is_empty();
        let border = if focused {
            theme::accent() // --focus-color: $blue
        } else {
            theme::surface_bg_alt() // --contrast-border in dark
        };

        // Build the display text with cursor
        let text_child: Div = if is_empty && !focused {
            // Placeholder (unfocused)
            div()
                .text_size(theme::z(12.0))
                .text_color(theme::text_muted())
                .child(placeholder.to_string())
        } else if is_empty && focused {
            // Placeholder with cursor (focused but empty)
            h_flex()
                .items_center()
                .text_size(theme::z(12.0))
                .child(
                    div()
                        .w(px(1.0))
                        .h(px(14.0))
                        .bg(theme::text_main())
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .text_color(theme::text_muted())
                        .child(placeholder.to_string()),
                )
        } else if focused {
            // Editable: show text with cursor and optional selection highlight
            let cursor_pos = cursor.min(value.len());
            let sel_highlight = theme::text_selection_bg();

            if let Some(sel_anchor) = selection {
                // Has selection: render before_sel + selected + after_sel with cursor
                let (sel_start, sel_end) = ordered_range(sel_anchor.min(value.len()), cursor_pos);
                let before_sel = &value[..sel_start];
                let selected = &value[sel_start..sel_end];
                let after_sel = &value[sel_end..];

                let nowrap = !multiline;
                let mut row = if multiline {
                    h_flex().items_start().text_size(theme::z(12.0)).flex_wrap()
                } else {
                    h_flex()
                        .items_center()
                        .overflow_x_hidden()
                        .text_size(theme::z(12.0))
                };

                if !before_sel.is_empty() {
                    let mut el = div()
                        .text_color(theme::text_main())
                        .child(before_sel.to_string());
                    if nowrap {
                        el = el.whitespace_nowrap();
                    }
                    row = row.child(el);
                }

                // Cursor at start of selection (if cursor < anchor)
                if cursor_pos == sel_start {
                    row = row.child(
                        div()
                            .w(px(1.0))
                            .h(px(14.0))
                            .bg(theme::text_main())
                            .flex_shrink_0(),
                    );
                }

                // Selected text with highlight
                if !selected.is_empty() {
                    let mut el = div()
                        .text_color(theme::on_accent())
                        .bg(sel_highlight)
                        .child(selected.to_string());
                    if nowrap {
                        el = el.whitespace_nowrap();
                    }
                    row = row.child(el);
                }

                // Cursor at end of selection (if cursor > anchor)
                if cursor_pos == sel_end {
                    row = row.child(
                        div()
                            .w(px(1.0))
                            .h(px(14.0))
                            .bg(theme::text_main())
                            .flex_shrink_0(),
                    );
                }

                if !after_sel.is_empty() {
                    let mut el = div()
                        .text_color(theme::text_main())
                        .child(after_sel.to_string());
                    if nowrap {
                        el = el.whitespace_nowrap();
                    }
                    row = row.child(el);
                }
                row
            } else {
                // No selection: just cursor
                let before = &value[..cursor_pos];
                let after = &value[cursor_pos..];

                if multiline {
                    let mut row = h_flex().items_start().text_size(theme::z(12.0));
                    row = row.child(
                        div()
                            .text_color(theme::text_main())
                            .child(before.to_string()),
                    );
                    row = row.child(
                        div()
                            .w(px(1.0))
                            .h(px(14.0))
                            .bg(theme::text_main())
                            .flex_shrink_0(),
                    );
                    row = row.child(
                        div()
                            .text_color(theme::text_main())
                            .child(after.to_string()),
                    );
                    row
                } else {
                    h_flex()
                        .items_center()
                        .overflow_x_hidden()
                        .text_size(theme::z(12.0))
                        .child(
                            div()
                                .text_color(theme::text_main())
                                .whitespace_nowrap()
                                .child(before.to_string()),
                        )
                        .child(
                            div()
                                .w(px(1.0))
                                .h(px(14.0))
                                .bg(theme::text_main())
                                .flex_shrink_0(),
                        )
                        .child(
                            div()
                                .text_color(theme::text_main())
                                .whitespace_nowrap()
                                .child(after.to_string()),
                        )
                }
            }
        } else {
            // Has text, not focused
            if multiline {
                div()
                    .text_size(theme::z(12.0))
                    .text_color(if enabled {
                        theme::text_main()
                    } else {
                        theme::text_muted()
                    })
                    .child(value.to_string())
            } else {
                div()
                    .text_size(theme::z(12.0))
                    .text_color(if enabled {
                        theme::text_main()
                    } else {
                        theme::text_muted()
                    })
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .child(value.to_string())
            }
        };

        let is_summary = id == "commit-summary-field";

        let mut field = if multiline {
            // Description: scrollable container with inner content that grows
            div()
                .id(SharedString::from(id.to_string()))
                .w_full()
                .h(px(80.0))
                .bg(theme::bg())
                .border_1()
                .border_color(border)
                .rounded_t(theme::z(theme::CORNER_RADIUS))
                .rounded_b_none()
                .border_b_0()
                .overflow_y_scroll()
                .when(!enabled, |s| s.opacity(0.65))
                .when(enabled, |s| {
                    s.track_focus(focus_handle)
                        .key_context("text-field")
                        .cursor_text()
                })
                .child(div().w_full().px(px(8.0)).py(px(6.0)).child(text_child))
        } else {
            // Summary: single line, vertically centered
            div()
                .id(SharedString::from(id.to_string()))
                .w_full()
                .h(px(25.0))
                .flex()
                .items_center()
                .bg(theme::bg())
                .border_1()
                .border_color(border)
                .px(px(8.0))
                .rounded(theme::z(theme::CORNER_RADIUS))
                .when(!enabled, |s| s.opacity(0.65))
                .when(enabled, |s| {
                    s.track_focus(focus_handle)
                        .key_context("text-field")
                        .cursor_text()
                })
                .child(text_child)
        };

        if !enabled {
            return field;
        }

        if is_summary {
            field = field
                .on_key_down(cx.listener(Self::handle_summary_key))
                .on_click(cx.listener(|app, evt: &ClickEvent, _win, cx| {
                    if evt.click_count() == 2 && !app.commit.summary.is_empty() {
                        // Select all on double-click
                        app.summary_selection = Some(0);
                        app.summary_cursor = app.commit.summary.len();
                        cx.notify();
                    }
                }));
        } else {
            field = field
                .on_key_down(cx.listener(Self::handle_description_key))
                .on_click(cx.listener(|app, evt: &ClickEvent, _win, cx| {
                    if evt.click_count() == 2 && !app.commit.body.is_empty() {
                        // Select all on double-click
                        app.description_selection = Some(0);
                        app.description_cursor = app.commit.body.len();
                        cx.notify();
                    }
                }));
        }

        field
    }

    // ------------------------------------------------------------------
    // Commit form (interactive)
    // ------------------------------------------------------------------

    fn render_commit_form_interactive(
        &self,
        branch_name: &str,
        summary_focused: bool,
        description_focused: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let file_count = self.commit_file_count();
        let commit_inputs_enabled = self.repo.snapshot.is_some();
        let can_generate_ai =
            self.repo.snapshot.is_some() && file_count > 0 && !self.commit.ai_in_flight;

        // Action bar buttons (below description, matching GitHub Desktop layout)
        let action_bar_btn = |id: &str, icon: IconName| -> Stateful<Div> {
            div()
                .id(SharedString::from(id.to_string()))
                .flex_shrink_0()
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover_bg()))
                .rounded(px(3.0))
                .w(px(18.0))
                .h(px(17.0))
                .items_center()
                .justify_center()
                .child(
                    Icon::new(icon)
                        .size(px(14.0))
                        .text_color(theme::text_muted()),
                )
        };

        let sparkle_button = div()
            .id("ai-generate-btn")
            .flex_shrink_0()
            .rounded(px(3.0))
            .w(px(18.0))
            .h(px(17.0))
            .items_center()
            .justify_center()
            .when(!can_generate_ai, |s| s.opacity(0.45))
            .when(can_generate_ai, |s| {
                s.cursor_pointer().hover(|s| s.bg(theme::hover_bg()))
            })
            .child(
                svg()
                    .path("icons/sparkles.svg")
                    .size(px(14.0))
                    .text_color(theme::text_muted()),
            )
            .on_click(cx.listener(|app, _evt, _win, cx| {
                if app.repo.snapshot.is_none()
                    || app.commit_file_count() == 0
                    || app.commit.ai_in_flight
                {
                    return;
                }
                app.generate_ai_commit(cx);
            }));

        let settings_button = action_bar_btn("commit-settings-btn", IconName::Settings).on_click(
            cx.listener(|app, _evt, window, cx| {
                app.open_settings_modal(Some(crate::ui::ui_state::SettingsSection::Ai), cx);
                app.activate_settings_field(
                    settings_modal::default_settings_field(
                        crate::ui::ui_state::SettingsSection::Ai,
                    ),
                    window,
                    cx,
                );
            }),
        );

        // Action bar — sits below the description textarea
        let action_bar = h_flex()
            .w_full()
            .h(px(26.0))
            .px(px(5.0))
            .items_center()
            .gap(px(2.0))
            .bg(theme::surface_bg())
            .border_1()
            .border_t_0()
            .border_color(theme::surface_bg_alt())
            .rounded_b(theme::z(theme::CORNER_RADIUS))
            .child(sparkle_button)
            .child(
                div()
                    .w(px(1.0))
                    .h(px(12.0))
                    .bg(theme::surface_bg_alt())
                    .mx(px(2.0)),
            )
            .child(settings_button);

        // Auto-placeholder: "Update filename" for a single included file, else generic
        let summary_placeholder = if self.commit.summary.is_empty() {
            self.default_commit_summary()
                .unwrap_or_else(|| "Summary (required)".to_string())
        } else {
            "Summary (required)".to_string()
        };

        // Summary field — editable single-line input
        let summary_field = self.render_text_field(
            "commit-summary-field",
            &self.commit.summary,
            &summary_placeholder,
            self.summary_cursor,
            self.summary_selection,
            summary_focused,
            false,
            &self.summary_focus,
            commit_inputs_enabled,
            cx,
        );

        // Description field — editable multi-line input (no bottom radius, action bar attaches)
        let description_field = self.render_text_field(
            "commit-description-field",
            &self.commit.body,
            "Description",
            self.description_cursor,
            self.description_selection,
            description_focused,
            true,
            &self.description_focus,
            commit_inputs_enabled,
            cx,
        );

        // Description + action bar grouped together (shared border)
        let description_group = v_flex().w_full().child(description_field).child(action_bar);

        let commit_label = if self.commit.ai_in_flight {
            "Generating commit details\u{2026}".to_string()
        } else if file_count > 0 {
            format!(
                "Commit {} to {branch_name}",
                crate::ui::labels::commit_files(file_count)
            )
        } else {
            format!("Commit to {branch_name}")
        };

        let can_commit = self.can_commit();
        let identity_warning = self
            .repo
            .snapshot
            .as_ref()
            .and_then(|_| self.missing_identity_message())
            .map(|message| {
                h_flex()
                    .id("commit-identity-warning")
                    .w_full()
                    .gap(px(8.0))
                    .items_center()
                    .px(px(8.0))
                    .py(px(7.0))
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .bg(theme::surface_bg())
                    .border_1()
                    .border_color(theme::warning())
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .size(px(13.0))
                            .text_color(theme::warning()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(12.0))
                            .text_color(theme::text_main())
                            .child(message),
                    )
                    .child(
                        div()
                            .id("commit-identity-settings")
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(3.0))
                            .bg(theme::surface_bg_alt())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::toolbar_hover_bg()))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme::text_main())
                                    .child("Git Settings"),
                            )
                            .on_click(cx.listener(|app, _evt, window, cx| {
                                let field = app.identity_settings_focus_field();
                                app.open_identity_settings_from_warning(cx);
                                app.activate_settings_field(field, window, cx);
                            })),
                    )
            });

        // Summary length hint (> 50 chars)
        let summary_hint = if self.commit.summary.len() > 50 {
            div()
                .flex_shrink_0()
                .child(
                    Icon::new(IconName::Info)
                        .size(px(12.0))
                        .text_color(theme::warning()),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };

        v_flex()
            .w_full()
            .border_t_1()
            .border_color(theme::toolbar_button_border())
            .bg(theme::panel_bg())
            .p(px(10.0))
            .gap(px(10.0))
            .child(
                h_flex()
                    .gap(px(4.0))
                    .child(summary_field)
                    .child(summary_hint),
            )
            .child(description_group)
            .when_some(identity_warning, |this, warning| this.child(warning))
            .child(
                Button::new("commit-btn")
                    .label(commit_label)
                    .small()
                    .disabled(!can_commit)
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(if can_commit {
                                theme::commit_button_bg()
                            } else {
                                theme::surface_bg_alt()
                            })
                            .foreground(if can_commit {
                                theme::commit_button_text()
                            } else {
                                theme::text_muted()
                            })
                            .hover(theme::commit_button_hover_bg())
                            .active(theme::commit_button_hover_bg()),
                    )
                    .on_click(cx.listener(|app, _evt, _win, cx| {
                        app.commit_all(cx);
                    })),
            )
    }

    // ------------------------------------------------------------------
    // Workspace
    // ------------------------------------------------------------------

    fn render_workspace(&self, view: Entity<Self>, cx: &mut Context<Self>) -> AnyElement {
        let sidebar_tab = self.nav.sidebar_tab;

        if self.repo.snapshot.is_none() {
            return h_resizable("workspace-panels")
                .child(
                    resizable_panel()
                        .child(crate::ui::sidebar::render_no_repository_state(&view, cx)),
                )
                .into_any_element();
        }

        // Determine the active file list and selected file based on tab.
        let (diffs, selected_file): (&[DiffEntry], Option<&str>) = match sidebar_tab {
            SidebarTab::Changes => {
                let diffs = self
                    .repo
                    .snapshot
                    .as_ref()
                    .map(|s| s.diffs.as_slice())
                    .unwrap_or(&[]);
                let sel = self.selection.selected_change.as_deref();
                (diffs, sel)
            }
            SidebarTab::History => {
                let diffs = self
                    .repo
                    .comparison
                    .as_ref()
                    .map(|comparison| comparison.diffs.as_slice())
                    .or_else(|| self.selection.commit_diffs.as_deref())
                    .unwrap_or(&[]);
                let sel = self.selection.selected_commit_file.as_deref();
                (diffs, sel)
            }
        };

        // Find the diff entry for the selected file.
        let selected_diff = selected_file.and_then(|path| diffs.iter().find(|d| d.path == path));

        // No local changes — show suggestion cards in the workspace
        if sidebar_tab == SidebarTab::Changes && diffs.is_empty() {
            let snapshot = self.repo.snapshot.as_ref();
            let ahead = snapshot.map(|s| s.repo.ahead).unwrap_or(0);
            let behind = snapshot.map(|s| s.repo.behind).unwrap_or(0);
            let remote = snapshot.and_then(|s| s.repo.remote_name.as_deref());
            let has_github_remote = snapshot.map(|s| s.repo.has_github_remote).unwrap_or(false);
            let content = h_resizable("workspace-panels")
                .child(
                    resizable_panel().child(crate::ui::sidebar::render_no_changes_state(
                        &view,
                        ahead,
                        behind,
                        remote,
                        has_github_remote,
                        cx,
                    )),
                )
                .into_any_element();
            return self.render_workspace_with_operation(content, cx);
        }

        // Show file list panel on History tab (Changes tab has sidebar file list)
        if sidebar_tab == SidebarTab::History {
            let commit_header = if let Some(comparison) = self.repo.comparison.as_ref() {
                crate::ui::compare_view::render_compare_detail_header(comparison, diffs, cx)
            } else {
                let selected_commit = self.repo.snapshot.as_ref().and_then(|snapshot| {
                    self.selection
                        .selected_commit
                        .as_deref()
                        .and_then(|oid| snapshot.history.iter().find(|commit| commit.oid == oid))
                });
                self.render_commit_detail_header(selected_commit, diffs, cx)
            };
            let file_list = self.render_commit_file_list(diffs, selected_file, sidebar_tab, cx);

            let content = v_flex()
                .size_full()
                .min_h_0()
                .child(commit_header)
                .child(
                    div().w_full().flex_1().min_h_0().child(
                        h_resizable("workspace-panels")
                            .child(
                                resizable_panel()
                                    .size(px(200.0))
                                    .size_range(px(120.0)..px(350.0))
                                    .child(file_list),
                            )
                            .child(resizable_panel().child(
                                crate::ui::workspace::render_workspace(
                                    None,
                                    selected_file,
                                    selected_diff,
                                    self.nav.diff_options.hide_whitespace_changes,
                                    self.nav.diff_options.show_side_by_side,
                                    false,
                                    &self.selection.selected_diff_lines,
                                    None, // History diffs are read-only, no expand controls
                                    &self.diff_list_history,
                                ),
                            )),
                    ),
                )
                .into_any_element();
            self.render_workspace_with_operation(content, cx)
        } else {
            let content = h_resizable("workspace-panels")
                .child(
                    resizable_panel().child(crate::ui::workspace::render_workspace(
                        self.repo_path(),
                        selected_file,
                        selected_diff,
                        self.nav.diff_options.hide_whitespace_changes,
                        self.nav.diff_options.show_side_by_side,
                        self.nav.show_diff_options_menu,
                        &self.selection.selected_diff_lines,
                        Some(&view),
                        &self.diff_list_changes,
                    )),
                )
                .into_any_element();
            self.render_workspace_with_operation(content, cx)
        }
    }

    fn render_workspace_with_operation(
        &self,
        content: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(operation) = self.repo.operation.as_ref() {
            v_flex()
                .size_full()
                .min_h_0()
                .child(crate::ui::conflict_banner::render_git_operation_banner(
                    operation, cx,
                ))
                .child(div().flex_1().min_h_0().child(content))
                .into_any_element()
        } else {
            content
        }
    }

    fn render_commit_detail_header(
        &self,
        commit: Option<&CommitInfo>,
        diffs: &[DiffEntry],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(commit) = commit else {
            return div()
                .id("history-commit-detail-header")
                .h(px(0.0))
                .into_any_element();
        };

        let oid = commit.oid.clone();
        let short_oid = commit.short_oid.clone();
        let (added, deleted) = diff_line_stats(diffs);

        h_flex()
            .id("history-commit-detail-header")
            .w_full()
            .h(px(58.0))
            .flex_shrink_0()
            .px(px(12.0))
            .py(px(8.0))
            .items_center()
            .justify_between()
            .bg(theme::panel_bg())
            .border_b_1()
            .border_color(theme::border())
            .child(
                v_flex()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(theme::z(13.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .whitespace_nowrap()
                                    .overflow_x_hidden()
                                    .child(commit.summary.clone()),
                            )
                            .children(if commit.is_head {
                                Some(Tag::primary().xsmall().child("HEAD"))
                            } else {
                                None
                            }),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(theme::z(12.0))
                                    .text_color(theme::text_muted())
                                    .whitespace_nowrap()
                                    .child(commit.author_name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(theme::z(12.0))
                                    .text_color(theme::text_muted())
                                    .child(short_oid.clone()),
                            )
                            .child(
                                div()
                                    .id("history-copy-sha-button")
                                    .px(px(5.0))
                                    .py(px(2.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::toolbar_hover_bg()))
                                    .on_click(cx.listener(move |app, _evt, _win, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            oid.clone(),
                                        ));
                                        app.messages.status_message =
                                            "Copied commit SHA.".to_string();
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_muted())
                                            .child("⧉"),
                                    ),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::success())
                            .child(format!("+{added}")),
                    )
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::danger())
                            .child(format!("-{deleted}")),
                    ),
            )
            .into_any_element()
    }

    fn render_commit_file_list(
        &self,
        diffs: &[DiffEntry],
        selected_file: Option<&str>,
        tab: SidebarTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity().clone();
        let diffs_snapshot: Vec<DiffEntry> = diffs.iter().cloned().collect();
        let sel = selected_file.map(String::from);

        let file_list = uniform_list(
            "commit-file-list",
            diffs_snapshot.len(),
            move |range, _win, _cx| {
                let sel = sel.clone();
                range
                    .map(|ix| {
                        let entry = &diffs_snapshot[ix];
                        let is_selected = sel.as_deref() == Some(entry.path.as_str());
                        let text_color = if is_selected {
                            theme::on_accent()
                        } else {
                            theme::text_main()
                        };

                        let path = entry.path.clone();
                        let id_path = stable_id_slug(&entry.path);
                        let vh = view.clone();

                        h_flex()
                            .id(SharedString::from(format!("commit-file-{id_path}")))
                            .w_full()
                            .h(theme::z(28.0))
                            .px(theme::z(10.0))
                            .items_center()
                            .bg(if is_selected {
                                theme::accent()
                            } else {
                                gpui::transparent_black()
                            })
                            // Blue left border for selected file
                            .border_l_2()
                            .border_color(if is_selected {
                                theme::accent()
                            } else {
                                gpui::transparent_black()
                            })
                            .cursor_pointer()
                            .hover(move |s| {
                                s.bg(if is_selected {
                                    theme::accent()
                                } else {
                                    theme::list_hover_bg()
                                })
                            })
                            .on_click(move |_evt, _win, cx| {
                                let path = path.clone();
                                vh.update(cx, |app, cx| {
                                    match tab {
                                        SidebarTab::Changes => {
                                            if app.selection.selected_change.as_deref()
                                                != Some(path.as_str())
                                            {
                                                app.selection.selected_diff_lines.clear();
                                            }
                                            app.selection.selected_change = Some(path);
                                        }
                                        SidebarTab::History => {
                                            app.selection.selected_commit_file = Some(path);
                                        }
                                    }
                                    cx.notify();
                                });
                            })
                            .child(
                                div().flex_1().overflow_x_hidden().child(
                                    div()
                                        .text_size(theme::z(12.0))
                                        .text_color(text_color)
                                        .whitespace_nowrap()
                                        .child(entry.path.clone()),
                                ),
                            )
                    })
                    .collect()
            },
        )
        .w_full()
        .with_sizing_behavior(ListSizingBehavior::Infer);

        v_flex()
            .size_full()
            .items_start()
            .bg(theme::panel_bg())
            .border_r_1()
            .border_color(theme::border())
            .child(
                h_flex()
                    .w_full()
                    .h(theme::z(32.0))
                    .px(theme::z(10.0))
                    .items_center()
                    .bg(theme::surface_bg_muted())
                    .border_b_1()
                    .border_color(theme::border())
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::text_muted())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::ui::labels::changed_files(diffs.len())),
                    ),
            )
            .child(
                div()
                    .id("commit-file-list-viewport")
                    .w_full()
                    .flex_1()
                    .overflow_hidden()
                    .child(file_list),
            )
    }

    // ------------------------------------------------------------------
    // Status bar
    // ------------------------------------------------------------------

    fn render_status_bar(&self) -> impl IntoElement {
        let branch = self
            .repo
            .snapshot
            .as_ref()
            .map(|s| s.repo.current_branch.as_str());
        let change_count = self
            .repo
            .snapshot
            .as_ref()
            .map(|s| s.changes.len())
            .unwrap_or(0);
        crate::ui::status_bar::render_status_bar(
            &self.messages.status_message,
            &self.messages.error_message,
            branch,
            change_count,
        )
    }
}

// ---------------------------------------------------------------------------
// Text input helpers
// ---------------------------------------------------------------------------

fn ordered_range(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Backdrop + panel for the Current Worktree picker, anchored under its
/// toolbar section. The panel sits at the left edge of the right-hand
/// cluster, which is exactly where the worktree section starts.
fn render_worktree_overlay(
    app: &GitSparkApp,
    filter_focused: bool,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let backdrop = div()
        .id("worktree-selector-backdrop")
        .absolute()
        .top(theme::z(theme::TOOLBAR_HEIGHT))
        .left_0()
        .w_full()
        .bottom_0()
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.show_worktree_selector = false;
            cx.notify();
        }));

    let panel = worktree_selector::render_worktree_selector_panel(app, filter_focused, cx)
        .id("worktree-selector-panel")
        .on_click(|_evt, _win, cx| cx.stop_propagation())
        .absolute()
        .top(theme::z(theme::TOOLBAR_HEIGHT))
        .left_0();

    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(backdrop)
        .child(panel)
}

fn prev_char_boundary(s: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut p = pos - 1;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

fn next_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos + 1;
    while p < s.len() && !s.is_char_boundary(p) {
        p += 1;
    }
    p
}

#[allow(dead_code)]
fn edit_string_field(
    value: &mut String,
    cursor: &mut usize,
    multiline: bool,
    event: &KeyDownEvent,
    cx: &mut Context<GitSparkApp>,
) {
    let ks = &event.keystroke;

    if ks.modifiers.secondary() {
        match ks.key.as_str() {
            "v" => {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        let text = if multiline {
                            text.to_string()
                        } else {
                            text.replace(['\n', '\r'], " ")
                        };
                        value.insert_str(*cursor, &text);
                        *cursor += text.len();
                        cx.notify();
                    }
                }
            }
            "a" => {
                *cursor = value.len();
                cx.notify();
            }
            _ => {}
        }
        return;
    }

    match ks.key.as_str() {
        "backspace" => {
            if *cursor > 0 {
                let new_pos = prev_char_boundary(value, *cursor);
                value.drain(new_pos..*cursor);
                *cursor = new_pos;
                cx.notify();
            }
        }
        "delete" => {
            if *cursor < value.len() {
                let end = next_char_boundary(value, *cursor);
                value.drain(*cursor..end);
                cx.notify();
            }
        }
        "left" => {
            if *cursor > 0 {
                *cursor = prev_char_boundary(value, *cursor);
                cx.notify();
            }
        }
        "right" => {
            if *cursor < value.len() {
                *cursor = next_char_boundary(value, *cursor);
                cx.notify();
            }
        }
        "home" => {
            *cursor = 0;
            cx.notify();
        }
        "end" => {
            *cursor = value.len();
            cx.notify();
        }
        "enter" if multiline => {
            value.insert(*cursor, '\n');
            *cursor += 1;
            cx.notify();
        }
        _ => {
            if let Some(ref ch) = ks.key_char {
                if !ks.modifiers.control
                    && (multiline || (!ch.contains('\n') && !ch.contains('\r')))
                {
                    value.insert_str(*cursor, ch);
                    *cursor += ch.len();
                    cx.notify();
                }
            }
        }
    }
}
