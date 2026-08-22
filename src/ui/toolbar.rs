use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::divider::Divider;
use gpui_component::{Icon, IconName, h_flex, v_flex};

use crate::ui::theme;
use crate::ui::theme::z;

// ---------------------------------------------------------------------------
// Section geometry (base values, scaled by theme::z())
// ---------------------------------------------------------------------------

const SECTION_ICON_SIZE: f32 = 16.0;
const SECTION_INNER_PADDING: f32 = 10.0;
const SECTION_GAP: f32 = 10.0;
const CARET_ICON_SIZE: f32 = 10.0;
const BADGE_PILL_RADIUS: f32 = 8.0;

pub const BRANCH_SECTION_WIDTH: f32 = 300.0;
pub const NETWORK_SECTION_WIDTH: f32 = 231.0;
pub const SECTION_DIVIDER_WIDTH: f32 = 1.0;
pub const NETWORK_DROPDOWN_WIDTH: f32 = 300.0;

/// Left edge of the network section, for anchoring its dropdown.
///
/// The branch and network sections are the only two children of the RIGHT
/// resizable panel — the worktree section lives in the left one, an entirely
/// separate positioned ancestor, since the panels were split. This used to
/// also add the worktree section's width, back when all three sections sat
/// in one row and the offset was measured from the window's own left edge
/// instead of the right panel's.
pub fn network_dropdown_left_offset() -> f32 {
    BRANCH_SECTION_WIDTH + SECTION_DIVIDER_WIDTH
}

// ---------------------------------------------------------------------------
// Icon source — either a built-in IconName or a custom SVG path
// ---------------------------------------------------------------------------

pub enum ToolbarIcon {
    Name(IconName),
    Svg(&'static str),
}

// ---------------------------------------------------------------------------
// Toolbar section builder (repo & branch)
// ---------------------------------------------------------------------------

pub fn render_toolbar_section(
    id: &str,
    icon: ToolbarIcon,
    description: &str,
    title: &str,
    is_open: bool,
    is_in_progress: bool,
    disabled: bool,
) -> Stateful<Div> {
    let title_row = h_flex().items_center().child(title_label(title));

    let bg = if is_open {
        theme::bg()
    } else {
        gpui::transparent_black()
    };

    let caret = if is_in_progress {
        div().flex_shrink_0().child(
            Icon::new(IconName::LoaderCircle)
                .size(z(CARET_ICON_SIZE))
                .text_color(theme::text_muted()),
        )
    } else if is_open {
        div().flex_shrink_0().child(
            Icon::new(IconName::ChevronUp)
                .size(z(CARET_ICON_SIZE))
                .text_color(theme::text_muted()),
        )
    } else {
        caret_icon()
    };

    h_flex()
        .id(SharedString::from(id.to_string()))
        .flex_1()
        .h_full()
        .items_center()
        .pl(z(SECTION_INNER_PADDING))
        .pr(z(SECTION_INNER_PADDING))
        .gap(z(SECTION_GAP))
        .bg(bg)
        .child(render_toolbar_icon(icon))
        .child(
            v_flex()
                .flex_1()
                .gap(z(2.0))
                .overflow_hidden()
                .child(description_label(description))
                .child(title_row),
        )
        .child(caret)
        .when(disabled, |style| style.opacity(0.55))
        .when(!disabled, |style| {
            style
                .cursor_pointer()
                .hover(|style| style.bg(theme::toolbar_hover_bg()))
        })
}

// ---------------------------------------------------------------------------
// Network section (split button)
// ---------------------------------------------------------------------------

pub fn render_network_parts(
    action_label: &str,
    ahead: usize,
    behind: usize,
    last_fetched: Option<&str>,
    is_in_flight: bool,
    show_dropdown: bool,
    disabled: bool,
) -> (Stateful<Div>, Stateful<Div>) {
    let description = last_fetched
        .map(|v| format!("Last fetched {v}"))
        .unwrap_or_else(|| "Never fetched".to_string());

    let badges: Option<Div> = if ahead > 0 || behind > 0 {
        let mut row = h_flex().gap(z(4.0));
        if ahead > 0 {
            row = row.child(count_badge(&format!("{ahead} \u{2191}")));
        }
        if behind > 0 {
            row = row.child(count_badge(&format!("{behind} \u{2193}")));
        }
        Some(row)
    } else {
        None
    };

    // Icon: always rotate-cw for fetch, ArrowUp for push, ArrowDown for pull.
    let is_fetch = {
        let lower = action_label.to_ascii_lowercase();
        !lower.starts_with("push") && !lower.starts_with("pull")
    };
    let is_push = action_label.to_ascii_lowercase().starts_with("push");

    // Icon: rotate-cw spins for fetch; push/pull nudge toward the direction
    // commits are actually travelling and get a progress rail underneath —
    // the arrow swap on its own gave no sense that anything was happening.
    let icon_element = {
        if is_fetch {
            // Custom rotate-cw SVG, with spin animation when fetching
            let svg_el = gpui::svg()
                .path("icons/rotate-cw.svg")
                .size(z(SECTION_ICON_SIZE))
                .text_color(theme::text_main());
            if is_in_flight {
                div()
                    .flex_shrink_0()
                    .child(svg_el.with_animation(
                        "spin",
                        Animation::new(Duration::from_secs(1)).repeat(),
                        |svg, delta| {
                            svg.with_transformation(Transformation::rotate(percentage(delta)))
                        },
                    ))
                    .into_any_element()
            } else {
                div().flex_shrink_0().child(svg_el).into_any_element()
            }
        } else {
            let svg_path = if is_push {
                "icons/arrow-up.svg"
            } else {
                "icons/arrow-down.svg"
            };
            let svg_el = gpui::svg()
                .path(svg_path)
                .size(z(SECTION_ICON_SIZE))
                .text_color(theme::text_main());
            if is_in_flight {
                // Positive nudges the arrow down (pull, toward the repo);
                // negative nudges it up (push, out of the repo). `bounce`
                // already shapes delta into a 0 -> 1 -> 0 sweep, so this is
                // one smooth there-and-back per cycle, not a sawtooth.
                let nudge: f32 = if is_push { -3.0 } else { 3.0 };
                div()
                    .flex_shrink_0()
                    .child(
                        svg_el.with_animation(
                            "network-nudge",
                            Animation::new(Duration::from_millis(700))
                                .repeat()
                                .with_easing(bounce(ease_in_out)),
                            move |svg, delta| {
                                svg.with_transformation(Transformation::translate(point(
                                    px(0.0),
                                    px(nudge * delta),
                                )))
                            },
                        ),
                    )
                    .into_any_element()
            } else {
                div().flex_shrink_0().child(svg_el).into_any_element()
            }
        }
    };

    let title_row = h_flex()
        .items_center()
        .gap(z(6.0))
        .child(title_label(action_label));

    let mut main_area = h_flex()
        .id("network-main")
        .flex_1()
        .h_full()
        .items_center()
        .pl(z(SECTION_INNER_PADDING))
        .gap(z(SECTION_GAP))
        .child(icon_element)
        .child(
            v_flex()
                .flex_1()
                .gap(z(2.0))
                .overflow_hidden()
                .child(title_row)
                .child(description_label(&description)),
        )
        .when(disabled, |style| style.opacity(0.55))
        .when(!disabled, |style| {
            style
                .cursor_pointer()
                .hover(|style| style.bg(theme::toolbar_hover_bg()))
        });

    // Badges at the top level of main_area for vertical centering
    if let Some(b) = badges {
        main_area = main_area.child(b).pr(z(SECTION_INNER_PADDING));
    }

    // Progress rail: an indeterminate sweep along the bottom edge while a
    // push or pull is running. Git gives no byte-level progress to report,
    // so this reads as "something is moving", not "X% done".
    if is_in_flight && !is_fetch {
        main_area = main_area.relative().child(
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .right_0()
                .h(px(2.0))
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .w(relative(0.4))
                        .rounded(px(1.0))
                        .bg(theme::accent())
                        .with_animation(
                            "network-rail-sweep",
                            Animation::new(Duration::from_millis(1000)).repeat(),
                            |bar, delta| bar.left(relative(-0.4 + delta * 1.4)),
                        ),
                ),
        );
    }

    let caret_bg = if show_dropdown {
        theme::bg()
    } else {
        gpui::transparent_black()
    };

    let caret_zone = div()
        .id("network-caret")
        .flex()
        .h_full()
        .flex_shrink_0()
        .px(z(8.0))
        .items_center()
        .justify_center()
        .bg(caret_bg)
        .border_l_1()
        .border_color(theme::toolbar_button_border())
        .child(
            Icon::new(if show_dropdown {
                IconName::ChevronUp
            } else {
                IconName::ChevronDown
            })
            .size(z(CARET_ICON_SIZE))
            .text_color(theme::text_muted()),
        )
        .when(disabled, |style| style.opacity(0.55))
        .when(!disabled, |style| {
            style
                .cursor_pointer()
                .hover(|style| style.bg(theme::toolbar_hover_bg()))
        });

    (main_area, caret_zone)
}

// ---------------------------------------------------------------------------
// Reusable micro-elements
// ---------------------------------------------------------------------------

fn render_toolbar_icon(icon: ToolbarIcon) -> Div {
    match icon {
        ToolbarIcon::Name(name) => div().flex_shrink_0().child(
            Icon::new(name)
                .size(z(SECTION_ICON_SIZE))
                .text_color(theme::text_main()),
        ),
        ToolbarIcon::Svg(path) => div().flex_shrink_0().child(
            gpui::svg()
                .path(path)
                .size(z(SECTION_ICON_SIZE))
                .text_color(theme::text_main()),
        ),
    }
}

fn caret_icon() -> Div {
    div().flex_shrink_0().child(
        Icon::new(IconName::ChevronDown)
            .size(z(CARET_ICON_SIZE))
            .text_color(theme::text_muted()),
    )
}

fn description_label(text: &str) -> Div {
    div()
        .text_size(z(theme::FONT_SIZE_SM))
        .text_color(theme::text_muted())
        .overflow_x_hidden()
        .whitespace_nowrap()
        .child(text.to_string())
}

fn title_label(text: &str) -> Div {
    div()
        .text_size(z(theme::FONT_SIZE))
        .text_color(theme::text_main())
        .font_weight(FontWeight::SEMIBOLD)
        .overflow_x_hidden()
        .whitespace_nowrap()
        .child(text.to_string())
}

fn count_badge(text: &str) -> Div {
    div()
        .px(z(6.0))
        .py(z(2.0))
        .rounded(z(BADGE_PILL_RADIUS))
        .bg(theme::toolbar_badge_bg())
        .text_size(z(theme::FONT_SIZE_XS))
        .text_color(theme::text_main())
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_string())
}

// ---------------------------------------------------------------------------
// Vertical divider
// ---------------------------------------------------------------------------

pub fn vertical_divider() -> Divider {
    Divider::vertical().color(theme::toolbar_button_border())
}
