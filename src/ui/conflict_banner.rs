use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{h_flex, v_flex};

use crate::models::{GitOperationKind, GitOperationState};
use crate::ui::app::GitSparkApp;
use crate::ui::ids::stable_id_slug;
use crate::ui::theme;

pub(crate) fn render_git_operation_banner(
    operation: &GitOperationState,
    cx: &mut Context<GitSparkApp>,
) -> AnyElement {
    let files = operation.conflicted_files.clone();
    let operation_name = operation.kind.name().to_string();
    let continue_text = match operation.kind {
        GitOperationKind::Merge => "Continue Merge",
        GitOperationKind::Rebase => "Continue Rebase",
    };
    let abort_text = match operation.kind {
        GitOperationKind::Merge => "Abort Merge",
        GitOperationKind::Rebase => "Abort Rebase",
    };
    let title = if let Some(target) = operation.target_branch.as_deref() {
        format!(
            "{}: {} → {}",
            operation.kind.title(),
            target,
            operation.current_branch
        )
    } else {
        operation.kind.title().to_string()
    };
    let next_step = if operation.can_continue {
        match operation.kind {
            GitOperationKind::Merge => {
                "All conflicted files are marked resolved. Continue the merge to finish."
            }
            GitOperationKind::Rebase => {
                "All conflicted files are marked resolved. Continue the rebase, skip this commit, or abort."
            }
        }
    } else {
        "Open each conflicted file, resolve the markers, then mark it resolved."
    };
    let view = cx.entity().clone();

    v_flex()
        .id("operation-conflict-banner")
        .w_full()
        .flex_shrink_0()
        .gap(px(10.0))
        .px(px(14.0))
        .py(px(12.0))
        .bg(theme::warning_bg())
        .border_b_1()
        .border_color(theme::warning())
        .child(
            h_flex()
                .items_start()
                .justify_between()
                .gap(px(16.0))
                .child(
                    v_flex()
                        .min_w_0()
                        .gap(px(4.0))
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "operation-{}-title",
                                    operation_name
                                )))
                                .text_size(theme::z(13.0))
                                .text_color(theme::text_main())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "operation-{}-message",
                                    operation_name
                                )))
                                .text_size(theme::z(12.0))
                                .text_color(theme::text_muted())
                                .child(operation.message.clone()),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "operation-{}-next-step",
                                    operation_name
                                )))
                                .text_size(theme::z(12.0))
                                .text_color(theme::text_main())
                                .child(next_step),
                        ),
                )
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .gap(px(8.0))
                        .child(operation_button(
                            "operation-continue",
                            continue_text,
                            operation.can_continue,
                            true,
                            cx.listener(|app, _evt, _win, cx| {
                                app.continue_git_operation(cx);
                            }),
                        ))
                        .children(if operation.kind == GitOperationKind::Rebase {
                            Some(operation_button(
                                "operation-skip",
                                "Skip",
                                true,
                                false,
                                cx.listener(|app, _evt, _win, cx| {
                                    app.skip_rebase_operation(cx);
                                }),
                            ))
                        } else {
                            None
                        })
                        .child(operation_button(
                            "operation-abort",
                            abort_text,
                            true,
                            false,
                            cx.listener(|app, _evt, _win, cx| {
                                app.abort_git_operation(cx);
                            }),
                        )),
                ),
        )
        .children(if files.is_empty() {
            Some(
                h_flex()
                    .id("operation-conflict-files-resolved")
                    .w_full()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .bg(theme::surface_bg())
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::success())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Resolved"),
                    )
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::text_muted())
                            .child("No conflicted files remain."),
                    ),
            )
        } else {
            Some(
                div()
                    .id("operation-conflict-files")
                    .max_h(px(96.0))
                    .overflow_y_scroll()
                    .child(
                        uniform_list("operation-conflict-file-list", files.len(), {
                            move |range, _win, _cx| {
                                range
                                    .map(|ix| {
                                        let file = &files[ix];
                                        let row_slug = stable_id_slug(&file.path);
                                        let editor_id = SharedString::from(format!(
                                            "operation-conflict-open-editor-{}",
                                            row_slug
                                        ));
                                        let reveal_id = SharedString::from(format!(
                                            "operation-conflict-reveal-{}",
                                            row_slug
                                        ));
                                        let resolved_id = SharedString::from(format!(
                                            "operation-conflict-mark-resolved-{}",
                                            row_slug
                                        ));
                                        let editor_path = file.path.clone();
                                        let reveal_path = file.path.clone();
                                        let resolved_path = file.path.clone();
                                        let editor_view = view.clone();
                                        let reveal_view = view.clone();
                                        let resolved_view = view.clone();
                                        h_flex()
                                            .id(SharedString::from(format!(
                                                "operation-conflict-file-{}",
                                                row_slug
                                            )))
                                            .w_full()
                                            .h(px(32.0))
                                            .items_center()
                                            .gap(px(8.0))
                                            .pr(px(4.0))
                                            .child(
                                                div()
                                                    .text_size(theme::z(11.0))
                                                    .text_color(theme::warning())
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(file.status.clone()),
                                            )
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .text_size(theme::z(12.0))
                                                    .text_color(theme::text_main())
                                                    .overflow_x_hidden()
                                                    .whitespace_nowrap()
                                                    .child(file.path.clone()),
                                            )
                                            .child(operation_row_button(
                                                editor_id,
                                                "Open",
                                                move |_evt, _win, cx| {
                                                    let path = editor_path.clone();
                                                    editor_view.update(cx, |app, cx| {
                                                        app.open_conflict_in_editor(path, cx);
                                                    });
                                                },
                                            ))
                                            .child(operation_row_button(
                                                reveal_id,
                                                "Reveal",
                                                move |_evt, _win, cx| {
                                                    let path = reveal_path.clone();
                                                    reveal_view.update(cx, |app, cx| {
                                                        app.reveal_conflict_file(path, cx);
                                                    });
                                                },
                                            ))
                                            .child(operation_row_button(
                                                resolved_id,
                                                "Mark Resolved",
                                                move |_evt, _win, cx| {
                                                    let path = resolved_path.clone();
                                                    resolved_view.update(cx, |app, cx| {
                                                        app.mark_conflict_resolved(path, cx);
                                                    });
                                                },
                                            ))
                                            .into_any_element()
                                    })
                                    .collect()
                            }
                        })
                        .with_sizing_behavior(ListSizingBehavior::Infer),
                    ),
            )
        })
        .into_any_element()
}

fn operation_button(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    primary: bool,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .px(theme::z(12.0))
        .py(theme::z(6.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .bg(if primary && enabled {
            theme::commit_button_bg()
        } else {
            theme::surface_bg()
        })
        .border_1()
        .border_color(if primary && enabled {
            theme::commit_button_bg()
        } else {
            theme::surface_bg_alt()
        })
        .when(enabled, |el| {
            el.cursor_pointer().hover(|s| {
                if primary {
                    s.bg(theme::commit_button_hover_bg())
                } else {
                    s.bg(theme::toolbar_hover_bg())
                }
            })
        })
        .when(enabled, |el| el.on_click(handler))
        .child(
            div()
                .text_size(theme::z(12.0))
                .text_color(if primary && enabled {
                    theme::commit_button_text()
                } else if enabled {
                    theme::text_main()
                } else {
                    theme::text_muted()
                })
                .child(label),
        )
        .into_any_element()
}

fn operation_row_button(
    id: SharedString,
    label: &'static str,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .px(theme::z(8.0))
        .py(theme::z(4.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .bg(theme::surface_bg())
        .border_1()
        .border_color(theme::surface_bg_alt())
        .cursor_pointer()
        .hover(|s| s.bg(theme::toolbar_hover_bg()))
        .on_click(handler)
        .child(
            div()
                .text_size(theme::z(11.0))
                .text_color(theme::text_main())
                .child(label),
        )
        .into_any_element()
}
