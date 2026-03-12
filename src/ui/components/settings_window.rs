use eframe::egui::{self, Align, Align2, Color32, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;

use crate::models::{AiProvider, AppSettings, RemoteModelOption};
use crate::ui::primitives::button::{styled_button, ButtonVariant, icon_button};
use crate::ui::primitives::dropdown::{dropdown_row, settings_field_frame, styled_dropdown};
use crate::ui::primitives::row::settings_nav_row;
use crate::ui::primitives::text_input::{styled_multiline, styled_password, styled_singleline};
use crate::ui::theme::{
    ACCENT_MUTED, BORDER, PANEL_BG, SURFACE_BG_MUTED, TEXT_MAIN, TEXT_MUTED,
    color_with_alpha,
};
use crate::ui::ui_state::{OpenRouterModelsState, SettingsSection};

use crate::models::GitIdentity;

pub enum SettingsAction {
    SaveGitConfig,
    SaveAiSettings,
    ChangeProvider(AiProvider),
    SelectOpenRouterModel(String),
    RetryOpenRouterModels,
    Close,
}

pub struct SettingsProps<'a> {
    pub open: bool,
    pub settings_section: &'a mut SettingsSection,
    pub status_message: &'a str,

    // Git settings
    pub identity: &'a mut GitIdentity,
    pub has_repo: bool,
    pub repo_path_display: Option<String>,

    // AI settings
    pub ai_settings: &'a mut AppSettings,
    pub openrouter_models: &'a OpenRouterModelsState,
    pub openrouter_model_filter: &'a mut String,
}

/// Returns (still_open, action)
pub fn render_settings_window(
    ctx: &egui::Context,
    props: &mut SettingsProps<'_>,
) -> (bool, Option<SettingsAction>) {
    let mut open = props.open;
    let mut action = None;
    let mut close_requested = false;

    let viewport_size = ctx
        .input(|input| input.viewport().inner_rect.map(|rect| rect.size()))
        .unwrap_or_else(|| Vec2::new(1280.0, 860.0));
    let window_width = (viewport_size.x - 56.0).clamp(680.0, 840.0);
    let window_height = (viewport_size.y - 72.0).clamp(520.0, 760.0);

    egui::Window::new("settings")
        .open(&mut open)
        .collapsible(false)
        .title_bar(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .fixed_size(Vec2::new(window_width, window_height))
        .frame(
            egui::Frame::default()
                .fill(PANEL_BG)
                .stroke(Stroke::new(1.0, color_with_alpha(BORDER, 230.0)))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(0)),
        )
        .show(ctx, |ui| {
            ui.set_min_size(Vec2::new(window_width, window_height));

            egui::Frame::default()
                .fill(PANEL_BG)
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(24, 18))
                .show(ui, |ui| {
                    // Header
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("Settings")
                                    .color(TEXT_MAIN)
                                    .size(24.0)
                                    .strong(),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(
                                    "Git configuration, AI commit preferences, and recent repositories.",
                                )
                                .color(TEXT_MUTED)
                                .size(12.0),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if icon_button(ui, icons::X, "Close").clicked() {
                                close_requested = true;
                            }
                        });
                    });

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(10.0);

                    let footer_height = 54.0;
                    let body_height = (ui.available_height() - footer_height).max(260.0);

                    // Body: nav + content
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), body_height),
                        egui::Layout::left_to_right(Align::Min),
                        |ui| {
                            // Nav column
                            ui.allocate_ui_with_layout(
                                Vec2::new(180.0, body_height),
                                egui::Layout::top_down(Align::Min),
                                |ui| {
                                    render_settings_nav(ui, props.settings_section);
                                },
                            );

                            let divider_x = ui.min_rect().left() + 196.0;
                            ui.painter().vline(
                                divider_x,
                                ui.max_rect().y_range(),
                                Stroke::new(1.0, BORDER),
                            );

                            ui.add_space(28.0);

                            // Content column
                            ui.allocate_ui_with_layout(
                                Vec2::new(ui.available_width(), body_height),
                                egui::Layout::top_down(Align::Min),
                                |ui| {
                                    egui::ScrollArea::vertical()
                                        .id_salt("settings_content_scroll")
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            match *props.settings_section {
                                                SettingsSection::Git => {
                                                    if let Some(a) = render_git_settings_section(
                                                        ui,
                                                        props.identity,
                                                        props.has_repo,
                                                        props.repo_path_display.as_deref(),
                                                    ) {
                                                        action = Some(a);
                                                    }
                                                }
                                                SettingsSection::Ai => {
                                                    if let Some(a) = render_ai_settings_section(
                                                        ui,
                                                        props.ai_settings,
                                                        props.openrouter_models,
                                                        props.openrouter_model_filter,
                                                    ) {
                                                        action = Some(a);
                                                    }
                                                }
                                            }
                                        });
                                },
                            );
                        },
                    );

                    // Footer
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if !props.status_message.is_empty() {
                            ui.label(
                                RichText::new(props.status_message)
                                    .color(TEXT_MUTED)
                                    .size(11.0),
                            );
                        }

                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if styled_button(ui, "Close", ButtonVariant::Secondary).clicked() {
                                close_requested = true;
                            }
                        });
                    });
                });
        });

    if close_requested {
        open = false;
    }

    (open, action)
}

fn render_settings_nav(ui: &mut egui::Ui, section: &mut SettingsSection) {
    ui.add_space(6.0);
    if settings_nav_row(
        ui,
        icons::GIT_BRANCH,
        "Git",
        "Identity and pull behavior",
        *section == SettingsSection::Git,
    )
    .clicked()
    {
        *section = SettingsSection::Git;
    }
    if settings_nav_row(
        ui,
        icons::SPARKLE,
        "AI Commit",
        "Model, endpoint, and prompt",
        *section == SettingsSection::Ai,
    )
    .clicked()
    {
        *section = SettingsSection::Ai;
    }
}

fn render_section_header(
    ui: &mut egui::Ui,
    eyebrow: &str,
    title: &str,
    description: &str,
) {
    ui.label(RichText::new(eyebrow).color(TEXT_MUTED).size(10.0).strong());
    ui.add_space(6.0);
    ui.label(RichText::new(title).color(TEXT_MAIN).size(20.0).strong());
    ui.add_space(6.0);
    ui.label(RichText::new(description).color(TEXT_MUTED).size(12.0));
}

fn render_git_settings_section(
    ui: &mut egui::Ui,
    identity: &mut GitIdentity,
    has_repo: bool,
    repo_path_display: Option<&str>,
) -> Option<SettingsAction> {
    let mut action = None;

    let description = repo_path_display
        .map(|p| format!("Applies to {p}"))
        .unwrap_or_else(|| "Open a repository to edit local Git configuration.".to_string());
    render_section_header(ui, "Git", "Repository Git configuration", &description);

    ui.add_space(8.0);
    ui.columns(2, |columns| {
        columns[0].label(RichText::new("User Name").color(TEXT_MUTED).size(11.0));
        columns[1].label(RichText::new("User Email").color(TEXT_MUTED).size(11.0));
        styled_singleline(&mut columns[0], &mut identity.user_name, "Jane Doe");
        styled_singleline(
            &mut columns[1],
            &mut identity.user_email,
            "jane@example.com",
        );
    });

    ui.add_space(16.0);
    ui.label(
        RichText::new("Repository Behavior")
            .color(TEXT_MAIN)
            .size(12.0)
            .strong(),
    );
    ui.add_space(8.0);
    ui.label(RichText::new("Default Branch").color(TEXT_MUTED).size(11.0));
    let default_branch = identity.default_branch.get_or_insert_with(String::new);
    styled_singleline(ui, default_branch, "main");

    ui.add_space(10.0);
    let mut pull_rebase = identity.pull_rebase.unwrap_or(false);
    let checkbox = ui.checkbox(&mut pull_rebase, "Use pull.rebase");
    if checkbox.hovered() {
        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
    }
    identity.pull_rebase = Some(pull_rebase);

    ui.add_space(18.0);
    if ui.add_enabled_ui(has_repo, |ui| {
        styled_button(ui, "Save Git Config", ButtonVariant::Primary)
    }).inner.clicked() {
        action = Some(SettingsAction::SaveGitConfig);
    }

    action
}

fn render_ai_settings_section(
    ui: &mut egui::Ui,
    ai_settings: &mut AppSettings,
    openrouter_models: &OpenRouterModelsState,
    openrouter_model_filter: &mut String,
) -> Option<SettingsAction> {
    let mut action = None;

    render_section_header(
        ui,
        "AI Commit",
        "Commit message generation",
        "These settings control the model and prompt used for AI commit suggestions.",
    );

    ui.add_space(8.0);
    ui.label(RichText::new("Provider").color(TEXT_MUTED).size(11.0));
    if let Some(a) = render_ai_provider_picker(ui, ai_settings) {
        action = Some(a);
    }

    ui.add_space(10.0);
    ui.label(RichText::new("Model").color(TEXT_MUTED).size(11.0));
    if ai_settings.ai.provider == AiProvider::OpenRouter {
        settings_field_frame(ui, |ui| {
            if let Some(a) = render_openrouter_model_picker(
                ui,
                ai_settings,
                openrouter_models,
                openrouter_model_filter,
            ) {
                action = Some(a);
            }
        });
    } else {
        settings_field_frame(ui, |ui| {
            styled_singleline(ui, &mut ai_settings.ai.model, "gpt-4.1-mini");
        });
    }

    ui.add_space(8.0);
    ui.label(
        RichText::new(format!(
            "Requests go to {}",
            ai_settings.ai.provider.default_endpoint()
        ))
        .color(TEXT_MUTED)
        .size(10.0),
    );

    ui.add_space(10.0);
    ui.label(RichText::new("API Key").color(TEXT_MUTED).size(11.0));
    styled_password(
        ui,
        &mut ai_settings.ai.api_key,
        ai_settings.ai.provider.api_key_hint(),
    );

    ui.add_space(10.0);
    ui.label(RichText::new("System Prompt").color(TEXT_MUTED).size(11.0));
    styled_multiline(
        ui,
        &mut ai_settings.ai.system_prompt,
        8,
        "Write a concise conventional commit message...",
    );

    ui.add_space(18.0);
    let save = ui
        .horizontal_centered(|ui| {
            styled_button(ui, "Save", ButtonVariant::Primary)
        })
        .inner;
    if save.clicked() {
        action = Some(SettingsAction::SaveAiSettings);
    }

    action
}

fn render_ai_provider_picker(
    ui: &mut egui::Ui,
    ai_settings: &mut AppSettings,
) -> Option<SettingsAction> {
    let mut action = None;
    let selected_text = ai_settings.ai.provider.display_name();
    styled_dropdown(
        ui,
        "ai_provider_picker",
        selected_text,
        ui.available_width(),
        220.0,
        |ui| {
            for provider in [AiProvider::OpenRouter, AiProvider::OpenAICompatible] {
                let label = provider.display_name();
                let is_selected = ai_settings.ai.provider == provider;
                let response = dropdown_row(ui, label, is_selected);

                if response.clicked() {
                    action = Some(SettingsAction::ChangeProvider(provider));
                    ui.memory_mut(|mem| mem.close_popup());
                }
            }
        },
    );
    action
}

fn render_openrouter_model_picker(
    ui: &mut egui::Ui,
    ai_settings: &mut AppSettings,
    openrouter_models: &OpenRouterModelsState,
    model_filter: &mut String,
) -> Option<SettingsAction> {
    let mut action = None;

    match openrouter_models {
        OpenRouterModelsState::Idle | OpenRouterModelsState::Loading => {
            egui::Frame::default()
                .fill(SURFACE_BG_MUTED)
                .stroke(Stroke::NONE)
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(10, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.label(
                            RichText::new("Loading OpenRouter models...").color(TEXT_MUTED),
                        );
                    });
                });
        }
        OpenRouterModelsState::Ready(models) => {
            let filter = model_filter.trim().to_ascii_lowercase();
            let selected_text = models
                .iter()
                .find(|model| model.id == ai_settings.ai.model)
                .map(|model| format!("{} ({})", model.name, model.id))
                .unwrap_or_else(|| {
                    if ai_settings.ai.model.trim().is_empty() {
                        "Select a model".to_string()
                    } else {
                        ai_settings.ai.model.clone()
                    }
                });

            ui.scope(|ui| {
                styled_dropdown(
                    ui,
                    "openrouter_model_picker",
                    &truncate_single_line(&selected_text, 48),
                    ui.available_width(),
                    320.0,
                    |ui| {
                        styled_singleline(ui, model_filter, "Search models...");
                        ui.add_space(8.0);

                        let filtered_models: Vec<&RemoteModelOption> = models
                            .iter()
                            .filter(|model| {
                                filter.is_empty()
                                    || model.id.to_ascii_lowercase().contains(&filter)
                                    || model.name.to_ascii_lowercase().contains(&filter)
                            })
                            .collect();

                        egui::ScrollArea::vertical().max_height(260.0).show(
                            ui,
                            |ui| {
                                if filtered_models.is_empty() {
                                    ui.label(
                                        RichText::new("No models match your search.")
                                            .color(TEXT_MUTED)
                                            .size(11.0),
                                    );
                                }

                                for model in filtered_models {
                                    let label = truncate_single_line(
                                        &format!("{} ({})", model.name, model.id),
                                        72,
                                    );
                                    let is_selected = ai_settings.ai.model == model.id;
                                    let response = dropdown_row(ui, &label, is_selected);

                                    if response.clicked() {
                                        action = Some(SettingsAction::SelectOpenRouterModel(
                                            model.id.clone(),
                                        ));
                                        ui.memory_mut(|mem| mem.close_popup());
                                    }
                                }
                            },
                        );
                    },
                );
            });
        }
        OpenRouterModelsState::Error(message) => {
            egui::Frame::default()
                .fill(SURFACE_BG_MUTED)
                .stroke(Stroke::NONE)
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(10, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(message).color(TEXT_MUTED).size(11.0));
                    ui.add_space(8.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Retry").color(Color32::WHITE),
                            )
                            .fill(ACCENT_MUTED)
                            .stroke(Stroke::NONE)
                            .corner_radius(6.0)
                            .min_size(Vec2::new(78.0, 28.0)),
                        )
                        .clicked()
                    {
                        action = Some(SettingsAction::RetryOpenRouterModels);
                    }
                });
        }
    }

    action
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
