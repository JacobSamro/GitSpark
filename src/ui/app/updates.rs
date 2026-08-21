//! The app's side of auto-update: run a cycle, show it, apply it.
//!
//! Everything that decides *what* happens lives in [`crate::update`]; this is
//! only the plumbing that gets it onto a worker thread and its results back
//! onto the UI thread. The split matters because the update logic is testable
//! without a window and this is not.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use gpui::{ClickEvent, Context, Window};

use crate::update::{UpdateState, apply, driver};

use super::GitSparkApp;

/// How long after launch the first check runs.
///
/// Startup is the busiest moment in the app's life — the repo is being read,
/// the status is refreshing — and an update check is the least urgent thing
/// happening. It waits.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(10);

/// How often the UI drains the worker's state channel.
const DRAIN_INTERVAL: Duration = Duration::from_millis(200);

impl GitSparkApp {
    /// Kick off a background check, then download whatever it finds.
    ///
    /// The download is deliberately silent and automatic: by the time the user
    /// is asked anything, the bytes are already on disk and verified, so
    /// "Restart to update" is instant rather than the start of a wait.
    pub(crate) fn start_update_check(&mut self, cx: &mut Context<Self>) {
        // A check already running is not worth queueing behind — the answer
        // would be the same, and two concurrent downloads would race for the
        // same staging path.
        if matches!(
            self.update_state,
            UpdateState::Checking | UpdateState::Downloading { .. }
        ) {
            return;
        }

        let (tx, rx) = channel::<UpdateState>();

        // A plain thread rather than the background executor: this blocks on
        // network and disk for as long as the download takes, and parking an
        // executor thread on it would starve the work that expects to be
        // short.
        std::thread::spawn(move || {
            let channel = driver::channel_for_this_build();
            driver::run(
                channel,
                Arc::new(move |state| {
                    // A send failure means the window is gone. Nothing to do
                    // but stop reporting.
                    let _ = tx.send(state);
                }),
            );
        });

        self.drain_update_states(rx, cx);
    }

    /// Poll the worker's channel and mirror each state onto the app.
    ///
    /// Ends when the worker drops its sender, which it does exactly once, on
    /// the terminal state.
    fn drain_update_states(&mut self, rx: Receiver<UpdateState>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(DRAIN_INTERVAL).await;

                // Take everything queued, keeping only the last: intermediate
                // download percentages the UI never drew are not worth a
                // repaint each.
                let mut latest = None;
                let disconnected = loop {
                    match rx.try_recv() {
                        Ok(state) => latest = Some(state),
                        Err(TryRecvError::Empty) => break false,
                        Err(TryRecvError::Disconnected) => break true,
                    }
                };

                if let Some(state) = latest {
                    let applied = this.update(cx, |app, cx| {
                        app.update_state = state;
                        cx.notify();
                    });
                    // The window is gone; the worker can keep running but
                    // there is nobody left to tell.
                    if applied.is_err() {
                        return;
                    }
                }

                if disconnected {
                    return;
                }
            }
        })
        .detach();
    }

    /// Schedule the one automatic check this session gets.
    ///
    /// Once, not on a timer: a desktop git client is opened and closed often
    /// enough that per-launch is frequent, and a background poll would keep
    /// hitting the network in a window the user left open for days.
    pub(crate) fn schedule_first_update_check(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FIRST_CHECK_DELAY).await;
            let _ = this.update(cx, |app, cx| app.start_update_check(cx));
        })
        .detach();
    }

    /// What clicking the title-bar indicator does.
    pub(crate) fn handle_update_indicator_click(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.update_state.clone() {
            UpdateState::ReadyToInstall { artifact, .. } => {
                self.install_and_restart(&artifact, cx);
            }
            // A failed check is worth one more try — most failures here are
            // transient network ones.
            UpdateState::Failed { .. } => {
                self.update_state = UpdateState::Idle;
                self.start_update_check(cx);
            }
            _ => {}
        }
    }

    /// Replace the installed application and restart into it.
    ///
    /// Runs on the UI thread on purpose. The install is a local file copy that
    /// takes well under a second, and doing it off-thread would mean the app
    /// could accept input — a commit, an edit — between the moment its own
    /// files are replaced and the moment it quits.
    fn install_and_restart(&mut self, artifact: &std::path::Path, cx: &mut Context<Self>) {
        let staging = match crate::update::download::staging_dir() {
            Ok(dir) => dir,
            Err(error) => {
                self.update_state = UpdateState::Failed {
                    message: format!("{error:#}"),
                };
                cx.notify();
                return;
            }
        };

        if let Err(error) = apply::install(artifact, &staging) {
            self.update_state = UpdateState::Failed {
                message: format!("{error:#}"),
            };
            cx.notify();
            return;
        }

        // Relaunch before quitting: if spawning the new process fails, the
        // user still has the running one, and is told why rather than being
        // left with no app at all.
        if let Err(error) = apply::relaunch() {
            self.update_state = UpdateState::Failed {
                message: format!("{error:#}"),
            };
            cx.notify();
            return;
        }

        cx.quit();
    }
}
