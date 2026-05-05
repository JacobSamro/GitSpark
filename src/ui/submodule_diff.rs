use gpui::*;
use gpui_component::{h_flex, v_flex};

use crate::models::DiffEntry;
use crate::ui::GitSparkApp;
use crate::ui::theme::z;
use crate::ui::{labels, theme};

pub(crate) fn render_submodule_diff_panel(
    file_path: &str,
    entry: &DiffEntry,
    view: Option<&Entity<GitSparkApp>>,
) -> Div {
    let mut panel = v_flex()
        .items_center()
        .gap(z(10.0))
        .text_size(z(14.0))
        .text_color(theme::text_muted())
        .child(
            div()
                .id("diff-submodule-panel")
                .text_color(theme::text_main())
                .child("Submodule changes"),
        )
        .child(submodule_summary(entry));

    if let Some(vh) = view {
        let open_view = vh.clone();
        let open_path = file_path.to_string();
        panel = panel.child(
            h_flex()
                .gap(z(8.0))
                .child(submodule_action_button(
                    "diff-submodule-open",
                    "Open Submodule",
                    move |_evt, _win, cx| {
                        let path = open_path.clone();
                        open_view.update(cx, |app, cx| {
                            app.open_submodule_repository(&path, cx);
                        });
                    },
                ))
                .child(submodule_action_button(
                    "diff-submodule-reveal",
                    labels::reveal_in_file_manager_menu(),
                    {
                        let reveal_view = vh.clone();
                        let reveal_path = file_path.to_string();
                        move |_evt, _win, cx| {
                            let path = reveal_path.clone();
                            reveal_view.update(cx, |app, cx| {
                                app.reveal_in_finder(&path);
                                cx.notify();
                            });
                        }
                    },
                )),
        );
    }

    panel
}

fn submodule_summary(entry: &DiffEntry) -> String {
    match (&entry.submodule_old_oid, &entry.submodule_new_oid) {
        (Some(old), Some(new)) => format!(
            "Changed from {} to {}.",
            short_oid(old.as_str()),
            short_oid(new.as_str())
        ),
        (Some(old), None) => format!("Previous commit: {}.", short_oid(old.as_str())),
        (None, Some(new)) => format!("New commit: {}.", short_oid(new.as_str())),
        (None, None) => "The referenced submodule commit changed.".to_string(),
    }
}

fn short_oid(oid: &str) -> &str {
    oid.get(..7).unwrap_or(oid)
}

fn submodule_action_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    h_flex()
        .id(id)
        .h(z(28.0))
        .px(z(12.0))
        .items_center()
        .justify_center()
        .rounded(z(theme::CORNER_RADIUS_SM))
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_bg())
        .text_size(z(12.0))
        .text_color(theme::text_main())
        .cursor_pointer()
        .hover(|style| style.bg(theme::toolbar_hover_bg()))
        .child(label)
        .on_click(on_click)
}
