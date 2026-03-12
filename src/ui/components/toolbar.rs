use std::time::Duration;

use eframe::egui::{self, Align, Align2, Color32, PopupCloseBehavior, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;

use crate::models::RepoSnapshot;
use crate::ui::domain_state::NetworkAction;
use crate::ui::primitives::dropdown::{dropdown_trigger, toolbar_dropdown};
use crate::ui::theme::{
    BORDER, PANEL_BG, SURFACE_BG, TEXT_MAIN, TEXT_MUTED, TOOLBAR_HEIGHT, TOOLBAR_INNER_HEIGHT,
    TOOLBAR_PADDING, blend_color, color_with_alpha,
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
        .exact_height(TOOLBAR_HEIGHT)
        .show_separator_line(false)
        .frame(
            egui::Frame::default()
                .fill(SURFACE_BG)
                .inner_margin(egui::Margin::from(TOOLBAR_PADDING))
                .stroke(Stroke::new(1.0, BORDER)),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.set_min_height(TOOLBAR_INNER_HEIGHT);
            ui.with_layout(egui::Layout::left_to_right(Align::Min), |ui| {
                // Repository block
                let repo_trigger = dropdown_trigger(
                    ui,
                    icons::FOLDER_NOTCH_OPEN,
                    "Current Repository",
                    props.repo_title,
                    238.0,
                );
                
                if repo_trigger.clicked() {
                    action = Some(ToolbarAction::ToggleRepoSelector);
                }

                let first_sep_x = ui.cursor().left() - 4.0;
                ui.painter().vline(
                    first_sep_x,
                    ui.max_rect().y_range(),
                    Stroke::new(1.0, BORDER),
                );

                // Branch block
                let branch_trigger = dropdown_trigger(
                    ui,
                    icons::GIT_BRANCH,
                    "Current Branch",
                    props.branch_title,
                    214.0,
                );

                toolbar_dropdown(
                    ui,
                    "toolbar_branch",
                    214.0,
                    &branch_trigger,
                    |ui| {
                        // Branch popup content
                         ui.label(
                             RichText::new("Switch Branch")
                                 .size(10.0)
                                 .color(TEXT_MUTED)
                                 .strong(),
                         );
                         ui.add_space(4.0);
                         // This part requires the snapshot to list branches, 
                         // but previously it was inside render_branch_dropdown.
                         // I need to move that logic here.
                         if let Some(snapshot) = props.snapshot {
                             for branch in &snapshot.branches {
                                 let is_current = branch.name == props.branch_title;
                                 if crate::ui::primitives::dropdown::dropdown_row(
                                     ui,
                                     &branch.name,
                                     is_current,
                                 )
                                 .clicked()
                                 {
                                     action = Some(ToolbarAction::SwitchBranch(branch.name.clone()));
                                     ui.close_menu();
                                 }
                             }
                         }
                    }
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
    ui.add_enabled_ui(false, |ui| {
        crate::ui::primitives::dropdown::dropdown_trigger(
            ui,
            icons::ARROW_CLOCKWISE,
            "Open a repository first",
            "Fetch origin",
            224.0,
        )
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
