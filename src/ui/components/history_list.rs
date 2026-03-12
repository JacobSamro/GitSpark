use eframe::egui::{self, RichText, Vec2};

use crate::models::CommitInfo;
use crate::ui::primitives::row::commit_row;
use crate::ui::theme::TEXT_MUTED;

pub struct HistoryListProps<'a> {
    pub history: &'a [CommitInfo],
    pub selected_commit: Option<&'a str>,
}

/// Returns the OID of a clicked commit, if any.
pub fn render_history_list(ui: &mut egui::Ui, props: &HistoryListProps<'_>) -> Option<String> {
    let mut clicked_oid = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;

            if props.history.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No history").color(TEXT_MUTED));
                });
                return;
            }

            for commit in props.history {
                let is_selected =
                    props.selected_commit == Some(commit.oid.as_str());
                if let Some(oid) = render_history_row(ui, commit, is_selected) {
                    clicked_oid = Some(oid);
                }
            }
        });

    clicked_oid
}

fn render_history_row(
    ui: &mut egui::Ui,
    commit: &CommitInfo,
    is_selected: bool,
) -> Option<String> {
    if commit_row(
        ui,
        &commit.oid,
        &commit.summary,
        &commit.author_name,
        &commit.date,
        commit.is_head,
        is_selected,
    )
    .clicked()
    {
        Some(commit.oid.clone())
    } else {
        None
    }
}
