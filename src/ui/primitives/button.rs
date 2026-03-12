use eframe::egui::{self, Color32, RichText, Stroke, Vec2};

use crate::ui::theme::{
    ACCENT, ACCENT_MUTED, BORDER, CORNER_RADIUS_SM, DANGER, SURFACE_BG, SURFACE_BG_ALT,
    TAB_HEIGHT, TEXT_MAIN, TEXT_MUTED, color_with_alpha,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
    Ghost,
    Icon,
}

impl ButtonVariant {
    fn fill(self) -> Color32 {
        match self {
            Self::Primary => ACCENT_MUTED,
            Self::Secondary => SURFACE_BG,
            Self::Danger => DANGER,
            Self::Ghost | Self::Icon => Color32::TRANSPARENT,
        }
    }

    fn stroke(self) -> Stroke {
        match self {
            Self::Primary | Self::Ghost | Self::Icon => Stroke::NONE,
            Self::Secondary => Stroke::new(1.0, BORDER),
            Self::Danger => Stroke::NONE,
        }
    }

    fn text_color(self) -> Color32 {
        match self {
            Self::Primary | Self::Danger => Color32::WHITE,
            Self::Secondary | Self::Ghost => TEXT_MAIN,
            Self::Icon => TEXT_MAIN,
        }
    }
}

pub fn styled_button(ui: &mut egui::Ui, label: &str, variant: ButtonVariant) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(variant.text_color()))
            .fill(variant.fill())
            .stroke(variant.stroke())
            .corner_radius(CORNER_RADIUS_SM),
    )
}

pub fn styled_button_sized(
    ui: &mut egui::Ui,
    label: &str,
    variant: ButtonVariant,
    size: Vec2,
) -> egui::Response {
    ui.add_sized(
        size,
        egui::Button::new(
            RichText::new(label)
                .color(variant.text_color())
                .strong(),
        )
        .fill(variant.fill())
        .stroke(variant.stroke())
        .corner_radius(CORNER_RADIUS_SM),
    )
}

pub fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(icon).color(TEXT_MAIN).size(14.0))
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CORNER_RADIUS_SM)
            .min_size(Vec2::new(28.0, 28.0)),
    )
    .on_hover_text(tooltip)
}

pub fn tab_button<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    current: &mut T,
    this_tab: T,
    label: &str,
    width: f32,
) {
    let active = *current == this_tab;
    let response = ui.add_sized(
        [width, TAB_HEIGHT],
        egui::Button::new(
            RichText::new(label)
                .color(if active { TEXT_MAIN } else { TEXT_MUTED })
                .strong(),
        )
        .fill(if active {
            SURFACE_BG
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
        .corner_radius(0.0),
    );

    if active {
        let underline_rect = egui::Rect::from_min_max(
            response.rect.left_bottom() - Vec2::new(0.0, 2.0),
            response.rect.right_bottom() + Vec2::new(0.0, 1.0),
        );
        ui.painter().rect_filled(underline_rect, 0.0, ACCENT);
    }

    if response.clicked() {
        *current = this_tab;
    }
}

pub fn nav_button(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    subtitle: &str,
    selected: bool,
) -> egui::Response {
    let fill = if selected {
        SURFACE_BG_ALT
    } else {
        Color32::TRANSPARENT
    };

    let response = egui::Frame::default()
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(CORNER_RADIUS_SM)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_min_height(36.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).color(TEXT_MUTED).size(16.0));
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(title)
                            .color(TEXT_MAIN)
                            .size(13.0)
                            .strong(),
                    );
                    if !subtitle.is_empty() {
                        ui.label(
                            RichText::new(subtitle)
                                .color(TEXT_MUTED)
                                .size(10.0),
                        );
                    }
                });
            });
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    if response.hovered() && !selected {
        ui.painter().rect_filled(
            response.rect,
            CORNER_RADIUS_SM,
            color_with_alpha(SURFACE_BG_ALT, 50.0),
        );
    }

    response
}
