//! The GitSpark component kit — the design system made executable.
//!
//! `design.md` defines the visual language; `theme.rs` holds its tokens; this
//! module holds the controls built from them. A `div()` chain in a view that
//! looks like a button, a dialog, a row, or a tag belongs here instead.
//!
//! ## Why the kit exists
//!
//! gpui-component's higher-level controls are largely unusable in this app
//! (design.md §10): `Input` and icon-bearing `Button` need `Root`, which
//! costs a blink-cursor timer and constant repaints; `Popover` needs
//! `Selectable`, which `Stateful<Div>` does not implement; `Badge` is an
//! absolute overlay, not an inline counter. So the app owns its control
//! layer, and gpui-component stays the engine underneath for the pieces that
//! do work — `TabBar`, `Divider`, `Icon`, `h_flex`/`v_flex`, resizable panels.
//!
//! ## Conventions
//!
//! - Builders return a bare `Stateful<Div>` / `Div` so the callsite attaches
//!   its own `.on_click(cx.listener(..))`. The kit owns appearance; views own
//!   behavior.
//! - Every interactive builder takes an explicit, content-derived id
//!   (design.md §9).
//! - Sizes are base values passed through [`theme::z`], so zoom reaches every
//!   control for free.

// A component library carries its full vocabulary ahead of its consumers:
// `status_tag`, `pill`, and `empty_state` describe patterns the app already
// draws by hand, and they exist here so the next migration is a swap rather
// than a design decision. `dead_code` is allowed per module for that reason —
// unused here means "not migrated yet", not "backlog".
#[allow(dead_code)]
pub mod button;
#[allow(dead_code)]
pub mod dialog;
#[allow(dead_code)]
pub mod empty_state;
#[allow(dead_code)]
pub mod surface;
#[allow(dead_code)]
pub mod tag;

#[allow(unused_imports)]
pub use button::{ButtonVariant, button, button_state, icon_button};
#[allow(unused_imports)]
pub use dialog::{dialog_body, dialog_footer, dialog_header, dialog_shell};
#[allow(unused_imports)]
pub use empty_state::{empty_state, section_header};
#[allow(unused_imports)]
pub use surface::Surface;
#[allow(unused_imports)]
pub use tag::{pill, status_tag};
