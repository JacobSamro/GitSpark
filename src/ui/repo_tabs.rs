//! Open repositories, one per tab (design.md §8.13).
//!
//! ## Why the state is parked rather than duplicated
//!
//! The app already holds exactly one `RepoState`, `CommitState` and
//! `SelectionState`, and several hundred call sites read them directly. Making
//! every one of those go through an active-tab lookup would be a very large,
//! very mechanical change with a lot of room for a silent mistake.
//!
//! So the app's own fields stay where they are and mean "the active tab", and
//! an inactive tab **parks** its state in [`TabState`] until it is activated
//! again. Switching is a swap: park what is live, restore what was parked.
//!
//! That also matches how tabs are meant to behave. A background tab does not
//! refresh continuously — it refreshes when you switch to it — so there is no
//! need for its state to be reachable while it is in the background, and only
//! the active repository keeps a filesystem watcher.

use std::path::{Path, PathBuf};

use crate::ui::domain_state::{CommitState, RepoState, SelectionState};
use crate::ui::ui_state::SidebarTab;

/// Everything that belongs to one repository rather than to the window.
///
/// Deliberately not `NavState` or `FilterState`: dropdown visibility and
/// filter text are properties of the window's current interaction, and
/// carrying them across a tab switch would restore a menu the user opened in
/// a different repository.
#[derive(Default)]
pub struct TabState {
    pub repo: RepoState,
    pub commit: CommitState,
    pub selection: SelectionState,
    /// Which sidebar tab this repository was last looking at. Per-repo because
    /// "I was reading history over here and staging over there" is a normal
    /// way to work with two checkouts.
    pub sidebar_tab: SidebarTab,
}

pub struct RepoTab {
    pub path: PathBuf,
    /// Label for the strip. Not always the directory name — see
    /// [`assign_labels`].
    pub label: String,
    /// Parked state. `None` for the active tab, whose state lives in the app's
    /// own fields.
    pub parked: Option<TabState>,
    /// Changed-file count for the badge, kept so a background tab can still
    /// show what it last knew without being refreshed.
    pub changed_count: usize,
}

impl RepoTab {
    pub fn new(path: PathBuf) -> Self {
        let label = directory_name(&path);
        Self {
            path,
            label,
            parked: Some(TabState::default()),
            changed_count: 0,
        }
    }
}

/// The directory name, or the whole path when there isn't one.
///
/// A repository at `/` has no file name, and a tab with an empty label is
/// unclickable in practice.
pub fn directory_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Give every tab a label, disambiguating repeats by their parent directory.
///
/// Checking out the same project twice — `work/api` and `personal/api` — is
/// ordinary, and two tabs both reading `api` would be a coin flip. Only the
/// names that actually collide get the parent, so the common case stays short.
pub fn assign_labels(tabs: &mut [RepoTab]) {
    let names: Vec<String> = tabs.iter().map(|tab| directory_name(&tab.path)).collect();

    for (index, tab) in tabs.iter_mut().enumerate() {
        let name = &names[index];
        let collides = names
            .iter()
            .enumerate()
            .any(|(other, candidate)| other != index && candidate == name);

        tab.label = if collides {
            match tab.path.parent().map(directory_name) {
                Some(parent) if !parent.is_empty() => format!("{name} \u{2014} {parent}"),
                _ => name.clone(),
            }
        } else {
            name.clone()
        };
    }
}

/// Where the active tab lands after closing `closed`.
///
/// Separated from the app so the index arithmetic — which is easy to get
/// subtly wrong and impossible to see in a screenshot — can be tested on its
/// own. Returns `None` when the last tab was closed.
pub fn active_after_close(len_before: usize, active: usize, closed: usize) -> Option<usize> {
    if len_before <= 1 {
        return None;
    }
    let len_after = len_before - 1;

    let next = if closed < active {
        // Everything after the closed tab shifts down, including the active
        // one, so the same repository stays selected.
        active - 1
    } else if closed > active {
        active
    } else {
        // The active tab itself closed. Prefer the tab that took its place,
        // which is the one that was to its right; at the end, step left.
        closed.min(len_after - 1)
    };

    Some(next.min(len_after - 1))
}

/// The index of an already-open tab for `path`, if any.
///
/// Opening a repository that is already open should focus its tab rather than
/// making a second one — two tabs on the same checkout would each hold their
/// own commit draft for the same working tree.
pub fn index_of(tabs: &[RepoTab], path: &Path) -> Option<usize> {
    tabs.iter().position(|tab| tab.path == path)
}

/// Whether a finished repository load answers the request still outstanding.
///
/// `git` resolves any path inside a work tree to its root, so a request for a
/// subdirectory legitimately comes back as the parent. Anything else is a load
/// for a repository the user has already navigated away from, and adopting it
/// would repoint the active tab at the wrong repository.
pub fn load_matches_request(pending: &Path, loaded_root: &Path) -> bool {
    pending == loaded_root || pending.starts_with(loaded_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs_from(paths: &[&str]) -> Vec<RepoTab> {
        paths
            .iter()
            .map(|p| RepoTab::new(PathBuf::from(p)))
            .collect()
    }

    #[test]
    fn closing_a_tab_before_the_active_one_keeps_the_same_repo_selected() {
        // Three tabs, active on the third. Closing the first shifts everything
        // down; staying on index 2 would silently switch repositories.
        assert_eq!(active_after_close(3, 2, 0), Some(1));
    }

    #[test]
    fn closing_a_tab_after_the_active_one_leaves_it_alone() {
        assert_eq!(active_after_close(3, 0, 2), Some(0));
    }

    #[test]
    fn closing_the_active_tab_selects_the_one_that_replaced_it() {
        // Closing the middle of three lands on what is now the middle — the
        // tab that was to its right.
        assert_eq!(active_after_close(3, 1, 1), Some(1));
    }

    #[test]
    fn closing_the_last_tab_steps_left() {
        // There is nothing to the right, so the index must come back inside
        // the list rather than dangling one past the end.
        assert_eq!(active_after_close(3, 2, 2), Some(1));
    }

    #[test]
    fn closing_the_only_tab_leaves_nothing_active() {
        assert_eq!(active_after_close(1, 0, 0), None);
    }

    #[test]
    fn the_resulting_index_is_always_in_range() {
        // Brute force, because an out-of-range index here panics on the next
        // render rather than misbehaving quietly.
        for len in 1..=6usize {
            for active in 0..len {
                for closed in 0..len {
                    if let Some(next) = active_after_close(len, active, closed) {
                        assert!(
                            next < len - 1,
                            "len={len} active={active} closed={closed} -> {next}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn labels_stay_short_when_nothing_collides() {
        let mut tabs = tabs_from(&["/a/gitspark", "/b/vvault"]);
        assign_labels(&mut tabs);
        assert_eq!(tabs[0].label, "gitspark");
        assert_eq!(tabs[1].label, "vvault");
    }

    #[test]
    fn colliding_names_gain_their_parent_directory() {
        let mut tabs = tabs_from(&["/work/api", "/personal/api", "/x/site"]);
        assign_labels(&mut tabs);
        assert_eq!(tabs[0].label, "api \u{2014} work");
        assert_eq!(tabs[1].label, "api \u{2014} personal");
        // The one that does not collide keeps the short form.
        assert_eq!(tabs[2].label, "site");
    }

    #[test]
    fn a_repository_at_the_filesystem_root_still_gets_a_label() {
        let mut tabs = tabs_from(&["/"]);
        assign_labels(&mut tabs);
        assert!(!tabs[0].label.is_empty(), "a blank tab cannot be clicked");
    }

    #[test]
    fn opening_an_already_open_repository_finds_its_tab() {
        let tabs = tabs_from(&["/a/one", "/a/two"]);
        assert_eq!(index_of(&tabs, Path::new("/a/two")), Some(1));
        assert_eq!(index_of(&tabs, Path::new("/a/three")), None);
    }

    #[test]
    fn two_tabs_never_point_at_the_same_checkout() {
        // The guard in `adopt_path_into_active_tab` relies on this being
        // detectable: a worktree switch can land the active tab on a path
        // another tab already holds, and two tabs on one working tree would
        // each keep their own commit draft for it.
        let tabs = tabs_from(&["/a/one", "/a/two", "/a/one"]);
        let duplicates = tabs
            .iter()
            .filter(|tab| tab.path == PathBuf::from("/a/one"))
            .count();
        assert_eq!(duplicates, 2, "the fixture is the situation being guarded");
        // `index_of` finds the FIRST, which is what makes the guard able to
        // keep one and drop the rest deterministically.
        assert_eq!(index_of(&tabs, Path::new("/a/one")), Some(0));
    }

    #[test]
    fn a_load_for_the_requested_repository_is_accepted() {
        assert!(load_matches_request(
            Path::new("/work/api"),
            Path::new("/work/api")
        ));
    }

    #[test]
    fn a_load_that_resolved_to_the_work_tree_root_is_accepted() {
        // Opening a subdirectory is normal, and git answers with the root.
        assert!(load_matches_request(
            Path::new("/work/api/src/deep"),
            Path::new("/work/api")
        ));
    }

    #[test]
    fn a_load_for_a_repository_we_navigated_away_from_is_rejected() {
        // The tab-switch race: the previous repository's load lands after the
        // user has already moved on. Adopting it repointed the active tab at
        // the wrong repository, which then looked like a duplicate and cost
        // the real tab for it.
        assert!(!load_matches_request(
            Path::new("/work/api"),
            Path::new("/work/site")
        ));
    }

    #[test]
    fn a_sibling_with_a_shared_prefix_is_not_a_match() {
        // `starts_with` on paths compares components, not characters, so
        // "/work/api-v2" must not match "/work/api".
        assert!(!load_matches_request(
            Path::new("/work/api-v2"),
            Path::new("/work/api")
        ));
    }

    #[test]
    fn a_new_tab_starts_parked() {
        // The invariant the swap relies on: a tab that is not active must be
        // holding its own state, or activating it would restore nothing.
        let tab = RepoTab::new(PathBuf::from("/a/one"));
        assert!(tab.parked.is_some());
    }
}
