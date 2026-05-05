use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{h_flex, v_flex};

use crate::models::{BranchComparison, DiffEntry};
use crate::ui::app::{GitSparkApp, diff_line_stats};
use crate::ui::theme;

pub(crate) fn render_compare_detail_header(
    comparison: &BranchComparison,
    diffs: &[DiffEntry],
    cx: &mut Context<GitSparkApp>,
) -> AnyElement {
    let (added, deleted) = diff_line_stats(diffs);
    let target = comparison.target_branch.clone();
    let can_merge = comparison.behind > 0;

    h_flex()
        .id("compare-detail-header")
        .w_full()
        .h(px(64.0))
        .flex_shrink_0()
        .px(px(12.0))
        .py(px(8.0))
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .bg(theme::panel_bg())
        .border_b_1()
        .border_color(theme::border())
        .child(
            v_flex()
                .min_w_0()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(theme::z(13.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .overflow_x_hidden()
                        .child(format!(
                            "Comparing {} against {}",
                            comparison.current_branch, comparison.target_branch
                        )),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(theme::text_muted())
                                .child(format!(
                                    "{} ahead, {} behind",
                                    comparison.ahead, comparison.behind
                                )),
                        )
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
                ),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .gap(px(8.0))
                .child(
                    div()
                        .id("compare-exit-button")
                        .px(theme::z(12.0))
                        .py(theme::z(6.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(theme::surface_bg())
                        .border_1()
                        .border_color(theme::surface_bg_alt())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::toolbar_hover_bg()))
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            app.repo.comparison = None;
                            app.selection.selected_commit_file = None;
                            app.selection.selected_commit = app
                                .repo
                                .snapshot
                                .as_ref()
                                .and_then(|snapshot| snapshot.history.first())
                                .map(|commit| commit.oid.clone());
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(theme::text_main())
                                .child("Exit Compare"),
                        ),
                )
                .child(
                    div()
                        .id("compare-merge-button")
                        .px(theme::z(12.0))
                        .py(theme::z(6.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(if can_merge {
                            theme::commit_button_bg()
                        } else {
                            theme::surface_bg()
                        })
                        .border_1()
                        .border_color(if can_merge {
                            theme::commit_button_bg()
                        } else {
                            theme::surface_bg_alt()
                        })
                        .when(can_merge, |el| {
                            el.cursor_pointer()
                                .hover(|s| s.bg(theme::commit_button_hover_bg()))
                        })
                        .on_click(cx.listener(move |app, _evt, _win, cx| {
                            if app
                                .repo
                                .comparison
                                .as_ref()
                                .is_none_or(|comparison| comparison.behind == 0)
                            {
                                return;
                            }
                            app.repo.merge_target = target.clone();
                            app.merge_branch(cx);
                        }))
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(if can_merge {
                                    theme::commit_button_text()
                                } else {
                                    theme::text_muted()
                                })
                                .child(format!(
                                    "Merge {} into {}",
                                    comparison.target_branch, comparison.current_branch
                                )),
                        ),
                ),
        )
        .into_any_element()
}
