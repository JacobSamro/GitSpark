use std::time::Duration;

use eframe::egui::{self, Align, Align2, Color32, PopupCloseBehavior, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;

use crate::models::{BranchInfo, RepoSnapshot};
use crate::ui::domain_state::NetworkAction;
use crate::ui::theme::{
    BORDER, PANEL_BG, SURFACE_BG, TEXT_MAIN, TEXT_MUTED, blend_color, color_with_alpha,
};

pub enum ToolbarAction {
    ToggleRepoSelector,
    SwitchBranch(String),
    RunNetworkAction(NetworkAction),
    FetchOrigin,
    PullOrigin,
    PushOrigin,
    OpenRepoDialog,
    RefreshRepo,
    OpenRepo(std::path::PathBuf),
}

pub struct ToolbarProps<'a> {
    pub repo_title: &'a str,
    pub branch_title: &'a str,
    pub snapshot: Option<&'a RepoSnapshot>,
    pub active_network_action: Option<NetworkAction>,
    pub recent_repos: &'a [std::path::PathBuf],
}

pub fn render_toolbar(
    ctx: &egui::Context,
    props: &ToolbarProps<'_>,
) -> Option<ToolbarAction> {
    let mut action = None;

    egui::TopBottomPanel::top("top_bar")
        .exact_height(68.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::default()
                .fill(SURFACE_BG)
                .inner_margin(egui::Margin::symmetric(12, 5))
                .stroke(Stroke::new(1.0, BORDER)),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.set_min_height(52.0);
            ui.with_layout(egui::Layout::left_to_right(Align::Min), |ui| {
                // Repository block
                ui.allocate_ui_with_layout(
                    Vec2::new(238.0, 52.0),
                    egui::Layout::top_down(Align::Min),
                    |ui| {
                        if let Some(a) = render_repository_trigger(
                            ui,
                            icons::FOLDER_NOTCH_OPEN,
                            "Current Repository",
                            props.repo_title,
                            238.0,
                        ) {
                            action = Some(a);
                        }
                    },
                );
                let first_sep_x = ui.cursor().left() - 4.0;
                ui.painter().vline(
                    first_sep_x,
                    ui.max_rect().y_range(),
                    Stroke::new(1.0, BORDER),
                );

                // Branch block
                ui.allocate_ui_with_layout(
                    Vec2::new(214.0, 52.0),
                    egui::Layout::top_down(Align::Min),
                    |ui| {
                        if let Some(a) = render_branch_dropdown(
                            ui,
                            props.branch_title,
                            props.snapshot,
                        ) {
                            action = Some(a);
                        }
                    },
                );
                let second_sep_x = ui.cursor().left() - 4.0;
                ui.painter().vline(
                    second_sep_x,
                    ui.max_rect().y_range(),
                    Stroke::new(1.0, BORDER),
                );

                // Network block
                ui.allocate_ui_with_layout(
                    Vec2::new(224.0, 52.0),
                    egui::Layout::top_down(Align::Min),
                    |ui| {
                        if let Some(a) = render_network_block(
                            ui,
                            props.snapshot,
                            props.active_network_action,
                        ) {
                            action = Some(a);
                        }
                    },
                );
            });
        });

    action
}

fn render_repository_trigger(
    ui: &mut egui::Ui,
    icon: &str,
    description: &str,
    title: &str,
    width: f32,
) -> Option<ToolbarAction> {
    let response = egui::Frame::default()
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(0.0)
        .inner_margin(egui::Margin::same(0))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(width, 52.0));
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.add_sized(
                    [18.0, 52.0],
                    egui::Label::new(RichText::new(icon).size(15.0).color(TEXT_MUTED)),
                );
                ui.add_space(12.0);
                render_toolbar_text_stack(
                    ui,
                    description,
                    title,
                    width - 76.0,
                    Some(icons::CARET_DOWN),
                );
            });
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    if response.clicked() {
        Some(ToolbarAction::ToggleRepoSelector)
    } else {
        None
    }
}

fn render_branch_dropdown(
    ui: &mut egui::Ui,
    branch_title: &str,
    snapshot: Option<&RepoSnapshot>,
) -> Option<ToolbarAction> {
    let mut action = None;
    let popup_id = ui.make_persistent_id("toolbar_branch");
    let width = 214.0;

    let response = egui::Frame::default()
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(0.0)
        .inner_margin(egui::Margin::same(0))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(width, 52.0));
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.add_sized(
                    [18.0, 52.0],
                    egui::Label::new(
                        RichText::new(icons::GIT_BRANCH).size(15.0).color(TEXT_MUTED),
                    ),
                );
                ui.add_space(12.0);
                render_toolbar_text_stack(
                    ui,
                    "Current Branch",
                    branch_title,
                    width - 76.0,
                    Some(icons::CARET_DOWN),
                );
            });
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    if response.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals;
        visuals.window_fill = PANEL_BG;
        visuals.window_stroke = Stroke::NONE;
        visuals.popup_shadow = egui::epaint::Shadow::NONE;

        egui::popup_below_widget(
            ui,
            popup_id,
            &response,
            PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(width.max(260.0));
                egui::Frame::default()
                    .fill(PANEL_BG)
                    .stroke(Stroke::NONE)
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Current Branch").small().color(TEXT_MUTED),
                        );
                        ui.add_space(6.0);

                        let branches = snapshot
                            .map(|s| s.branches.as_slice())
                            .unwrap_or(&[]);

                        if branches.is_empty() {
                            ui.label(
                                RichText::new("No branches available").color(TEXT_MUTED),
                            );
                            return;
                        }

                        for branch in branches.iter().filter(|b| !b.is_remote) {
                            let label = if branch.is_current {
                                format!("✓ {}", branch.name)
                            } else {
                                branch.name.clone()
                            };

                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(label).color(TEXT_MAIN),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(Stroke::NONE)
                                    .min_size(Vec2::new(ui.available_width(), 24.0)),
                                )
                                .clicked()
                            {
                                if !branch.is_current {
                                    action = Some(ToolbarAction::SwitchBranch(
                                        branch.name.clone(),
                                    ));
                                }
                                ui.close_menu();
                            }
                        }

                        let remote_branches: Vec<&BranchInfo> =
                            branches.iter().filter(|b| b.is_remote).collect();
                        if !remote_branches.is_empty() {
                            ui.separator();
                            ui.label(
                                RichText::new("Remote Branches")
                                    .small()
                                    .color(TEXT_MUTED),
                            );
                            for branch in remote_branches {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(branch.name.clone())
                                                .color(TEXT_MUTED),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE)
                                        .min_size(Vec2::new(ui.available_width(), 24.0)),
                                    )
                                    .clicked()
                                {
                                    action = Some(ToolbarAction::SwitchBranch(
                                        branch.name.clone(),
                                    ));
                                    ui.close_menu();
                                }
                            }
                        }
                    });
            },
        );
    });

    action
}

fn render_network_block(
    ui: &mut egui::Ui,
    snapshot: Option<&RepoSnapshot>,
    active_action: Option<NetworkAction>,
) -> Option<ToolbarAction> {
    let Some(snapshot) = snapshot else {
        render_disabled_network_block(ui);
        return None;
    };

    let mut action = None;
    let popup_id = ui.make_persistent_id("toolbar_network");
    let remote_name = snapshot
        .repo
        .remote_name
        .as_deref()
        .unwrap_or("origin");
    let primary_action = NetworkAction::from_snapshot(snapshot);
    let is_action_active = active_action == Some(primary_action);
    let any_action_active = active_action.is_some();

    let action_anim = ui.ctx().animate_bool_with_time(
        ui.make_persistent_id(("toolbar_network_action", primary_action)),
        is_action_active,
        0.18,
    );
    let animation_time = ui.input(|input| input.time) as f32;
    let action_pulse = if is_action_active {
        ui.ctx()
            .request_repaint_after(Duration::from_millis(16));
        ((animation_time * 7.0).sin() * 0.5 + 0.5) * action_anim
    } else {
        0.0
    };

    let title = if is_action_active {
        format!("{}...", primary_action.pending_title(remote_name))
    } else {
        primary_action.title(remote_name)
    };
    let description = snapshot
        .repo
        .last_fetched
        .as_ref()
        .map(|value| format!("Last fetched {value}"))
        .unwrap_or_else(|| "Never fetched".to_string());

    let block = egui::Frame::default()
        .fill(color_with_alpha(
            SURFACE_BG,
            if is_action_active {
                132.0 + action_pulse * 28.0
            } else {
                0.0
            },
        ))
        .stroke(if is_action_active {
            Stroke::new(
                1.0,
                blend_color(BORDER, TEXT_MUTED, 0.28 + action_pulse * 0.18),
            )
        } else {
            Stroke::NONE
        })
        .corner_radius(0.0)
        .inner_margin(egui::Margin::same(0))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(224.0, 52.0));
            ui.horizontal(|ui| {
                let main = ui
                    .allocate_ui_with_layout(
                        Vec2::new(185.0, 52.0),
                        egui::Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.add_space(12.0);
                            let (icon_rect, _) = ui.allocate_exact_size(
                                Vec2::new(18.0, 52.0),
                                egui::Sense::hover(),
                            );
                            let icon_offset = if is_action_active {
                                (-1.0 + (animation_time * 10.0).sin() * 1.5)
                                    * action_anim
                            } else {
                                0.0
                            };
                            ui.painter().text(
                                egui::pos2(
                                    icon_rect.left(),
                                    icon_rect.center().y + icon_offset,
                                ),
                                Align2::LEFT_CENTER,
                                primary_action.icon(),
                                egui::FontId::proportional(15.0 + action_anim * 0.5),
                                blend_color(TEXT_MUTED, TEXT_MAIN, action_anim),
                            );
                            ui.add_space(12.0);
                            render_network_text_stack(
                                ui,
                                &description,
                                &title,
                                snapshot.repo.ahead,
                                snapshot.repo.behind,
                                143.0,
                                action_anim,
                                action_pulse,
                            );
                        },
                    )
                    .response
                    .interact(egui::Sense::click());
                let main = if any_action_active {
                    main
                } else {
                    main.on_hover_cursor(egui::CursorIcon::PointingHand)
                };

                let divider_x = ui.min_rect().left() + 185.0;
                ui.painter().vline(
                    divider_x,
                    ui.max_rect().y_range(),
                    Stroke::new(1.0, BORDER),
                );
                let arrow_button = egui::Button::new(
                    RichText::new(icons::CARET_DOWN)
                        .size(11.0)
                        .color(TEXT_MUTED),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE);
                let arrow = ui.add_enabled(!any_action_active, arrow_button);
                let arrow = if any_action_active {
                    arrow
                } else {
                    arrow.on_hover_cursor(egui::CursorIcon::PointingHand)
                };

                (main, arrow)
            })
            .inner
        });
    let (main_response, arrow_response) = block.inner;

    if !any_action_active && main_response.clicked() {
        action = Some(ToolbarAction::RunNetworkAction(primary_action));
    }

    if arrow_response.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals;
        visuals.window_fill = PANEL_BG;
        visuals.window_stroke = Stroke::NONE;
        visuals.popup_shadow = egui::epaint::Shadow::NONE;

        egui::popup_below_widget(
            ui,
            popup_id,
            &block.response,
            PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(246.0);
                if let Some(a) = render_network_menu(
                    ui,
                    snapshot,
                    remote_name,
                    primary_action,
                    any_action_active,
                ) {
                    action = Some(a);
                }
            },
        );
    });

    action
}

fn render_disabled_network_block(ui: &mut egui::Ui) {
    egui::Frame::default()
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(0.0)
        .inner_margin(egui::Margin::same(0))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(224.0, 52.0));
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.add_sized(
                    [18.0, 52.0],
                    egui::Label::new(
                        RichText::new(icons::ARROW_CLOCKWISE)
                            .size(15.0)
                            .color(TEXT_MUTED),
                    ),
                );
                ui.add_space(12.0);
                render_toolbar_text_stack(
                    ui,
                    "Open a repository first",
                    "Fetch origin",
                    143.0,
                    Some(icons::CARET_DOWN),
                );
            });
        });
}

fn render_network_menu(
    ui: &mut egui::Ui,
    snapshot: &RepoSnapshot,
    remote_name: &str,
    primary_action: NetworkAction,
    any_action_active: bool,
) -> Option<ToolbarAction> {
    let mut action = None;

    ui.label(RichText::new("Remote").small().color(TEXT_MUTED));
    ui.add_space(6.0);

    let primary_label = if any_action_active {
        format!("{}...", primary_action.pending_title(remote_name))
    } else {
        primary_action.title(remote_name)
    };

    if ui
        .add_enabled(!any_action_active, egui::Button::new(primary_label))
        .clicked()
    {
        action = Some(ToolbarAction::RunNetworkAction(primary_action));
        ui.close_menu();
    }

    if primary_action != NetworkAction::Fetch
        && ui
            .add_enabled(
                !any_action_active,
                egui::Button::new(format!("Fetch {remote_name}")),
            )
            .clicked()
    {
        action = Some(ToolbarAction::FetchOrigin);
        ui.close_menu();
    }

    if snapshot.repo.behind > 0
        && primary_action != NetworkAction::Pull
        && ui
            .add_enabled(
                !any_action_active,
                egui::Button::new(format!("Pull {remote_name}")),
            )
            .clicked()
    {
        action = Some(ToolbarAction::PullOrigin);
        ui.close_menu();
    }

    if snapshot.repo.ahead > 0
        && primary_action != NetworkAction::Push
        && ui
            .add_enabled(
                !any_action_active,
                egui::Button::new(format!("Push {remote_name}")),
            )
            .clicked()
    {
        action = Some(ToolbarAction::PushOrigin);
        ui.close_menu();
    }

    ui.separator();
    ui.label(
        RichText::new(format!(
            "{}  {}↑ {}↓",
            remote_name, snapshot.repo.ahead, snapshot.repo.behind
        ))
        .small()
        .color(TEXT_MUTED),
    );

    action
}

// --- Text rendering helpers ---

fn render_toolbar_text_stack(
    ui: &mut egui::Ui,
    description: &str,
    title: &str,
    width: f32,
    trailing_icon: Option<&str>,
) {
    let chevron_width = if trailing_icon.is_some() { 18.0 } else { 0.0 };
    let text_width = (width - chevron_width).max(0.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 52.0), egui::Sense::hover());
    let painter = ui.painter();
    let text_left = rect.left();
    let text_top = rect.top() + 9.0;

    painter.text(
        egui::pos2(text_left, text_top),
        Align2::LEFT_TOP,
        truncate_single_line(description, 30),
        egui::FontId::proportional(10.0),
        TEXT_MUTED,
    );
    painter.text(
        egui::pos2(text_left, text_top + 13.0),
        Align2::LEFT_TOP,
        truncate_single_line(title, 24),
        egui::FontId::proportional(12.5),
        TEXT_MAIN,
    );

    if let Some(icon) = trailing_icon {
        painter.text(
            egui::pos2(rect.left() + text_width + 4.0, rect.top() + 20.0),
            Align2::LEFT_TOP,
            icon,
            egui::FontId::proportional(11.0),
            TEXT_MUTED,
        );
    }
}

fn render_network_text_stack(
    ui: &mut egui::Ui,
    description: &str,
    title: &str,
    ahead: usize,
    behind: usize,
    width: f32,
    active_t: f32,
    pulse: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 52.0), egui::Sense::hover());
    let painter = ui.painter();
    let text_left = rect.left();
    let text_top = rect.top() + 9.0;
    let description_color = blend_color(TEXT_MUTED, TEXT_MAIN, active_t * 0.35);
    let title_color = blend_color(TEXT_MAIN, Color32::WHITE, active_t * 0.35);
    let indicator_color = blend_color(TEXT_MUTED, TEXT_MAIN, active_t * 0.5);

    painter.text(
        egui::pos2(text_left, text_top),
        Align2::LEFT_TOP,
        truncate_single_line(description, 26),
        egui::FontId::proportional(10.0),
        description_color,
    );
    painter.text(
        egui::pos2(text_left, text_top + 13.0),
        Align2::LEFT_TOP,
        truncate_single_line(title, 18),
        egui::FontId::proportional(12.5),
        title_color,
    );

    if active_t > 0.0 {
        let progress_width = 28.0 + pulse * 28.0;
        let progress_rect = egui::Rect::from_min_size(
            egui::pos2(text_left, rect.bottom() - 10.0),
            Vec2::new(progress_width, 2.0),
        );
        painter.rect_filled(
            progress_rect,
            1.0,
            color_with_alpha(TEXT_MAIN, 100.0 + pulse * 52.0),
        );
    }

    let mut indicator_x = text_left + 92.0;
    if ahead > 0 {
        painter.text(
            egui::pos2(indicator_x, text_top + 13.0),
            Align2::LEFT_TOP,
            format!("{ahead}{}", icons::ARROW_UP),
            egui::FontId::proportional(11.0),
            indicator_color,
        );
        indicator_x += 22.0;
    }
    if behind > 0 {
        painter.text(
            egui::pos2(indicator_x, text_top + 13.0),
            Align2::LEFT_TOP,
            format!("{behind}{}", icons::ARROW_DOWN),
            egui::FontId::proportional(11.0),
            indicator_color,
        );
    }
}

fn truncate_single_line(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let mut chars = trimmed.chars();
    let shortened: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}
