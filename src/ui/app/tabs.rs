//! Switching between open repositories.
//!
//! The swap is the whole idea, and it is explained in [`crate::ui::repo_tabs`]:
//! the app's own `repo` / `commit` / `selection` fields *are* the active tab,
//! and an inactive tab parks a copy of them until it is activated again. That
//! keeps every existing call site working unchanged and means only the active
//! repository ever holds a filesystem watcher.

use std::path::{Path, PathBuf};

use gpui::Context;

use crate::ui::repo_tabs::{self, RepoTab, TabState};

use crate::ui::ui_state::{ActiveDialog, BranchSelectorMode};

use super::GitSparkApp;

impl GitSparkApp {
    // -----------------------------------------------------------------
    // Parking
    // -----------------------------------------------------------------

    /// Lift the live state out of the app's fields.
    ///
    /// `mem::take` rather than clone: this runs on every tab switch and the
    /// snapshot inside `RepoState` carries the whole file and history list.
    fn park_live_state(&mut self) -> TabState {
        TabState {
            repo: std::mem::take(&mut self.repo),
            commit: std::mem::take(&mut self.commit),
            selection: std::mem::take(&mut self.selection),
            sidebar_tab: self.nav.sidebar_tab,
        }
    }

    /// Put a parked state back into the app's fields.
    fn install_state(&mut self, state: TabState) {
        self.repo = state.repo;
        self.commit = state.commit;
        self.selection = state.selection;
        self.nav.sidebar_tab = state.sidebar_tab;

        // Cursors index into the commit draft that just arrived, so they have
        // to be re-derived or they can point past the end of the new text.
        self.summary_cursor = self.commit.summary.len();
        self.summary_selection = None;
        self.description_cursor = self.commit.body.len();
        self.description_selection = None;
    }

    /// Park the active tab's state so it survives being switched away from.
    fn park_active(&mut self) {
        let live = self.park_live_state();
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.changed_count = changed_count(&live);
            tab.parked = Some(live);
        }
    }

    // -----------------------------------------------------------------
    // Opening
    // -----------------------------------------------------------------

    /// Open `path` in a new tab, or focus its tab if it is already open.
    ///
    /// Two tabs on one checkout would each keep their own commit draft for the
    /// same working tree, and only one of them could be right.
    pub(crate) fn open_repo_in_tab(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(existing) = repo_tabs::index_of(&self.tabs, &path) {
            self.activate_tab(existing, cx);
            return;
        }

        if !self.tabs.is_empty() {
            self.park_active();
        }

        self.tabs.push(RepoTab::new(path.clone()));
        self.active_tab = self.tabs.len() - 1;
        repo_tabs::assign_labels(&mut self.tabs);

        // The new tab starts parked; it becomes live by being emptied out.
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.parked = None;
        }
        self.install_state(TabState::default());

        self.open_repo(path);
        self.persist_open_tabs();
        cx.notify();
    }

    /// Make sure the active tab exists and points at `path`.
    ///
    /// Called when a repository is loaded into the *current* tab rather than a
    /// new one — a worktree switch, a fresh clone, or the first repository at
    /// launch. Those all mean "this tab is now looking at that", not "open
    /// another tab".
    pub(crate) fn adopt_path_into_active_tab(&mut self, path: &Path) {
        match self.tabs.get_mut(self.active_tab) {
            Some(tab) => {
                if tab.path != path {
                    tab.path = path.to_path_buf();
                }
            }
            None => {
                self.tabs.push(RepoTab::new(path.to_path_buf()));
                self.active_tab = 0;
                if let Some(tab) = self.tabs.get_mut(0) {
                    tab.parked = None;
                }
            }
        }
        // Pointing this tab at a path another tab already holds would leave
        // two tabs on one checkout, each with its own commit draft for the
        // same working tree. That happens for real when a worktree switch
        // lands on a repository that is already open, so drop the now
        // redundant one rather than letting the pair drift apart.
        self.drop_duplicate_tabs();

        repo_tabs::assign_labels(&mut self.tabs);
        let count = self
            .repo
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.changes.len())
            .unwrap_or(0);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.changed_count = count;
        }
    }

    /// Remove any tab other than the active one that points at the active
    /// tab's path, keeping the active index pointing at the same tab.
    fn drop_duplicate_tabs(&mut self) {
        let Some(active_path) = self.tabs.get(self.active_tab).map(|tab| tab.path.clone()) else {
            return;
        };

        let mut index = 0;
        self.tabs.retain(|tab| {
            let keep = index == self.active_tab || tab.path != active_path;
            index += 1;
            keep
        });

        // Recompute rather than adjust: `retain` can remove several tabs from
        // either side of the active one, and an off-by-one here silently
        // selects the wrong repository.
        self.active_tab = self
            .tabs
            .iter()
            .position(|tab| tab.path == active_path)
            .unwrap_or(0);
    }

    // -----------------------------------------------------------------
    // Switching and closing
    // -----------------------------------------------------------------

    pub(crate) fn activate_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() || index == self.active_tab {
            return;
        }

        self.park_active();

        // Dropdowns and dialogs belong to the interaction that opened them, in
        // the repository it was opened in. Carrying an open branch picker into
        // a different repository would list the wrong branches.
        self.dismiss_transient_ui();

        self.active_tab = index;
        let parked = self
            .tabs
            .get_mut(index)
            .and_then(|tab| tab.parked.take())
            .unwrap_or_default();
        self.install_state(parked);

        // The watcher follows the active tab: a background repository is not
        // watched, which is why switching to one has to refresh it.
        self.stop_repo_watch();
        if let Some(path) = self.tabs.get(index).map(|tab| tab.path.clone()) {
            // Reload either way. A background tab has no watcher, so whatever
            // it last knew is as old as the moment it was switched away from.
            self.open_repo(path);
        }

        self.persist_open_tabs();
        cx.notify();
    }

    pub(crate) fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }

        let next = repo_tabs::active_after_close(self.tabs.len(), self.active_tab, index);

        // Park first so the removal below cannot drop live state that belongs
        // to a tab which is staying open.
        if index != self.active_tab {
            self.park_active();
        }

        self.tabs.remove(index);
        repo_tabs::assign_labels(&mut self.tabs);

        match next {
            Some(next) => {
                self.active_tab = next;
                let parked = self
                    .tabs
                    .get_mut(next)
                    .and_then(|tab| tab.parked.take())
                    .unwrap_or_default();
                self.install_state(parked);
                self.stop_repo_watch();
                if let Some(path) = self.tabs.get(next).map(|tab| tab.path.clone()) {
                    self.open_repo(path);
                }
            }
            None => {
                // The last tab closed. The window stays and shows the empty
                // state — `⌘W` never means "quit".
                self.active_tab = 0;
                self.stop_repo_watch();
                self.install_state(TabState::default());
                self.dismiss_transient_ui();
                self.messages.status_message = "No repository open.".to_string();
            }
        }

        self.persist_open_tabs();
        cx.notify();
    }

    /// Move `delta` tabs from the active one, wrapping at both ends.
    pub(crate) fn cycle_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.tabs.len();
        if len < 2 {
            return;
        }
        let current = self.active_tab as isize;
        let next = (current + delta).rem_euclid(len as isize) as usize;
        self.activate_tab(next, cx);
    }

    /// Reopen the tabs the last session had, and load the one that was in
    /// front.
    ///
    /// Only the active tab is loaded. The rest stay as empty tabs until they
    /// are switched to, which is the same rule that governs them for the rest
    /// of the session — the alternative is N git processes racing each other
    /// at launch, which is exactly the cost this feature has to avoid.
    ///
    /// Falls back to the most recent repository so an install that predates
    /// tabs still opens where it left off.
    pub(crate) fn restore_open_tabs(&mut self, settings: &crate::models::AppSettings) {
        let mut paths: Vec<PathBuf> = settings
            .open_repos
            .iter()
            .filter(|path| path.is_dir())
            .cloned()
            .collect();

        if paths.is_empty() {
            paths.extend(settings.recent_repos.first().cloned());
        }
        if paths.is_empty() {
            return;
        }

        self.tabs = paths.into_iter().map(RepoTab::new).collect();
        repo_tabs::assign_labels(&mut self.tabs);

        self.active_tab = settings
            .active_repo
            .unwrap_or(0)
            .min(self.tabs.len().saturating_sub(1));

        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.parked = None;
            let path = tab.path.clone();
            self.open_repo(path);
        }
    }

    /// The `+` button: show the repository list, and open whatever is picked
    /// as a new tab.
    ///
    /// The list is the same panel the toolbar's repository section used to
    /// open — it did not disappear with that section, it moved here, which is
    /// also why it still offers "Add" for a directory that is not in it yet.
    pub(crate) fn show_repo_list_for_new_tab(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.nav.show_repo_selector = !self.nav.show_repo_selector;
        if self.nav.show_repo_selector {
            window.focus(&self.repo_filter_focus);
        }
        cx.notify();
    }

    // -----------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------

    /// Remember which repositories are open, so a relaunch restores them.
    ///
    /// The whole point of tabs is not having to reopen things.
    pub(crate) fn persist_open_tabs(&mut self) {
        let open: Vec<PathBuf> = self.tabs.iter().map(|tab| tab.path.clone()).collect();
        let active = self.active_tab.min(self.tabs.len().saturating_sub(1));
        if self.settings.open_repos == open && self.settings.active_repo == Some(active) {
            return;
        }
        self.settings.open_repos = open;
        self.settings.active_repo = Some(active);
        self.persist_settings();
    }
}

impl GitSparkApp {
    /// Close anything that belongs to the interaction rather than the repo.
    ///
    /// An open branch picker carried across a switch would list the previous
    /// repository's branches, and a dialog would act on the wrong checkout.
    fn dismiss_transient_ui(&mut self) {
        self.nav.show_repo_selector = false;
        self.nav.show_branch_selector = false;
        self.nav.show_worktree_selector = false;
        self.nav.show_network_dropdown = false;
        self.nav.show_diff_options_menu = false;
        self.nav.branch_selector_mode = BranchSelectorMode::Switch;
        self.nav.active_dialog = ActiveDialog::None;
    }
}

/// Changed-file count carried on the tab badge.
fn changed_count(state: &TabState) -> usize {
    state
        .repo
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.changes.len())
        .unwrap_or(0)
}
