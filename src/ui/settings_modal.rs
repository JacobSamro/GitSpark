use gpui::*;
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::divider::Divider;
use gpui_component::radio::Radio;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::spinner::Spinner;
use gpui_component::switch::Switch;
use gpui_component::{Disableable, h_flex, v_flex};

use crate::models::{AiProvider, RemoteModelOption, UpdateChannel, UpdateStatus};
use crate::ui::app::{GitSparkApp, SettingsAction};
use crate::ui::theme;
use crate::ui::ui_state::{OpenRouterModelsState, SettingsSection};
use crate::updater;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsField {
    GitUserName,
    GitUserEmail,
    GitDefaultBranch,
    AiModel,
    AiApiKey,
    AiSystemPrompt,
    OpenRouterModelFilter,
}

pub(crate) struct SettingsModalState {
    pub focus: FocusHandle,
    pub active_field: Option<SettingsField>,
    pub git_user_name_cursor: usize,
    pub git_user_email_cursor: usize,
    pub git_default_branch_cursor: usize,
    pub ai_model_cursor: usize,
    pub ai_api_key_cursor: usize,
    pub ai_system_prompt_cursor: usize,
    pub openrouter_model_filter_cursor: usize,
}

impl SettingsModalState {
    pub fn new(cx: &mut Context<GitSparkApp>) -> Self {
        Self {
            focus: cx.focus_handle(),
            active_field: Some(default_settings_field(SettingsSection::Git)),
            git_user_name_cursor: 0,
            git_user_email_cursor: 0,
            git_default_branch_cursor: 0,
            ai_model_cursor: 0,
            ai_api_key_cursor: 0,
            ai_system_prompt_cursor: 0,
            openrouter_model_filter_cursor: 0,
        }
    }
}

pub(crate) fn default_settings_field(section: SettingsSection) -> SettingsField {
    match section {
        SettingsSection::Git => SettingsField::GitUserName,
        SettingsSection::Ai => SettingsField::AiModel,
        SettingsSection::Appearance
        | SettingsSection::Updates
        | SettingsSection::Integrations => SettingsField::GitUserName,
    }
}

pub(crate) fn render_settings_modal(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let repo_scope = app
        .repo
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.repo.path.display().to_string());
    let has_repo = app.repo.snapshot.is_some();
    let status_text = if !app.messages.error_message.is_empty() {
        Some((app.messages.error_message.as_str(), theme::danger()))
    } else if !app.messages.status_message.is_empty() {
        Some((app.messages.status_message.as_str(), theme::text_muted()))
    } else {
        None
    };

    let section_action = match app.nav.settings_section {
        SettingsSection::Git => Some(
            render_primary_button("settings-save-git", "Save Git Config", has_repo, cx)
                .on_click(cx.listener(|app, _evt, _window, cx| {
                    app.handle_settings_action(SettingsAction::SaveGitConfig, cx);
                }))
                .into_any_element(),
        ),
        SettingsSection::Ai => Some(
            render_primary_button("settings-save-ai", "Save AI Settings", true, cx)
                .on_click(cx.listener(|app, _evt, _window, cx| {
                    app.handle_settings_action(SettingsAction::SaveAiSettings, cx);
                }))
                .into_any_element(),
        ),
        SettingsSection::Appearance | SettingsSection::Updates | SettingsSection::Integrations => None,
    };

    let content = match app.nav.settings_section {
        SettingsSection::Git => {
            render_git_section(app, window, repo_scope.as_deref(), cx).into_any_element()
        }
        SettingsSection::Ai => render_ai_section(app, window, cx).into_any_element(),
        SettingsSection::Appearance => {
            v_flex()
                .flex_1()
                .p(px(20.0))
                .gap(px(16.0))
                .child(
                    div()
                        .text_size(px(16.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::BOLD)
                        .child("Appearance"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme::text_muted())
                        .child("Theme: Dark (GitHub)"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme::text_muted())
                        .child("Additional themes coming soon."),
                )
                .into_any_element()
        }
        SettingsSection::Updates => render_updates_section(app, cx).into_any_element(),
        SettingsSection::Integrations => {
            v_flex()
                .flex_1()
                .p(px(20.0))
                .gap(px(16.0))
                .child(
                    div()
                        .text_size(px(16.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::BOLD)
                        .child("Integrations"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme::text_muted())
                        .child("External editor and shell preferences coming soon."),
                )
                .into_any_element()
        }
    };

    let panel = v_flex()
        .id("settings-modal-panel")
        .track_focus(&app.settings_modal.focus)
        .key_context("settings-modal")
        .on_key_down(cx.listener(GitSparkApp::handle_settings_key))
        .occlude()
        .absolute()
        .top(theme::z(40.0))
        .right(theme::z(40.0))
        .bottom(theme::z(40.0))
        .left(theme::z(40.0))
        .bg(theme::panel_bg())
        .border_1()
        .border_color(theme::border())
        .rounded(theme::z(theme::CORNER_RADIUS))
        .overflow_hidden()
        .child(render_header(cx))
        .child(Divider::horizontal().color(theme::border()))
        .child(
            h_flex()
                .flex_1()
                .overflow_hidden()
                .child(render_nav(app, cx))
                .child(Divider::vertical().color(theme::border()).h_full())
                .child(
                    div().flex_1().overflow_y_scrollbar().child(
                        v_flex()
                            .w_full()
                            .p(theme::z(24.0))
                            .gap(theme::z(18.0))
                            .child(content),
                    ),
                ),
        )
        .child(Divider::horizontal().color(theme::border()))
        .child(
            h_flex()
                .w_full()
                .min_h(theme::z(64.0))
                .px(theme::z(24.0))
                .py(theme::z(14.0))
                .justify_between()
                .items_center()
                .gap(theme::z(12.0))
                .child(status_text.map_or_else(
                    || div().flex_1().into_any_element(),
                    |(message, color)| {
                        div()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(theme::z(11.0))
                                    .text_color(color)
                                    .child(message.to_string()),
                            )
                            .into_any_element()
                    },
                ))
                .child(
                    h_flex()
                        .gap(theme::z(10.0))
                        .items_center()
                        .child(
                            render_secondary_button("settings-close-footer", "Close", cx).on_click(
                                cx.listener(|app, _evt, _window, cx| {
                                    app.handle_settings_action(SettingsAction::Close, cx);
                                }),
                            ),
                        )
                        .children(section_action),
                ),
        );

    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(
            div()
                .id("settings-modal-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(theme::with_alpha(theme::bg(), 0.72))
                .on_click(cx.listener(|app, _evt, _window, cx| {
                    app.handle_settings_action(SettingsAction::Close, cx);
                })),
        )
        .child(panel)
}

fn render_header(cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    h_flex()
        .w_full()
        .px(theme::z(24.0))
        .py(theme::z(20.0))
        .justify_between()
        .items_start()
        .gap(theme::z(16.0))
        .child(
            v_flex()
                .gap(theme::z(6.0))
                .child(
                    div()
                        .text_size(theme::z(22.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::BOLD)
                        .child("Settings"),
                )
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_muted())
                        .child(
                            "Git configuration, AI commit preferences, and repository defaults.",
                        ),
                ),
        )
        .child(
            render_secondary_button("settings-close-header", "Close", cx).on_click(cx.listener(
                |app, _evt, _window, cx| {
                    app.handle_settings_action(SettingsAction::Close, cx);
                },
            )),
        )
}

fn render_nav(app: &GitSparkApp, cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    v_flex()
        .w(theme::z(240.0))
        .h_full()
        .p(theme::z(18.0))
        .gap(theme::z(10.0))
        .child(render_nav_radio(
            "settings-nav-git",
            "Git",
            "Identity and pull behavior",
            app.nav.settings_section == SettingsSection::Git,
            SettingsSection::Git,
            cx,
        ))
        .child(render_nav_radio(
            "settings-nav-ai",
            "AI Commit",
            "Provider, model, and prompt",
            app.nav.settings_section == SettingsSection::Ai,
            SettingsSection::Ai,
            cx,
        ))
        .child(render_nav_radio(
            "settings-nav-appearance",
            "Appearance",
            "Theme and visual preferences",
            app.nav.settings_section == SettingsSection::Appearance,
            SettingsSection::Appearance,
            cx,
        ))
        .child(render_nav_radio(
            "settings-nav-updates",
            "Updates",
            "Update channel and version",
            app.nav.settings_section == SettingsSection::Updates,
            SettingsSection::Updates,
            cx,
        ))
        .child(render_nav_radio(
            "settings-nav-integrations",
            "Integrations",
            "External editor and shell",
            app.nav.settings_section == SettingsSection::Integrations,
            SettingsSection::Integrations,
            cx,
        ))
}

fn render_nav_radio(
    id: &'static str,
    label: &'static str,
    description: &'static str,
    selected: bool,
    section: SettingsSection,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    Radio::new(id)
        .checked(selected)
        .label(label)
        .on_click(cx.listener(move |app, _checked: &bool, window, cx| {
            app.nav.settings_section = section;
            let field = if section == SettingsSection::Ai
                && app.settings.ai.provider == AiProvider::OpenRouter
            {
                SettingsField::OpenRouterModelFilter
            } else {
                default_settings_field(section)
            };
            app.activate_settings_field(field, window, cx);
        }))
        .w_full()
        .p(theme::z(12.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(if selected {
            theme::accent()
        } else {
            theme::border()
        })
        .bg(if selected {
            theme::surface_bg()
        } else {
            theme::surface_bg_muted()
        })
        .child(
            div()
                .text_size(theme::z(11.0))
                .text_color(theme::text_muted())
                .child(description),
        )
}

fn render_git_section(
    app: &GitSparkApp,
    window: &Window,
    repo_scope: Option<&str>,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let description = repo_scope
        .map(|path| format!("Applies to {path}"))
        .unwrap_or_else(|| "Open a repository to edit local Git configuration.".to_string());

    v_flex()
        .w_full()
        .gap(theme::z(20.0))
        .child(render_section_header(
            "Git",
            "Repository Git configuration",
            &description,
        ))
        .child(
            h_flex()
                .w_full()
                .gap(theme::z(14.0))
                .items_start()
                .child(div().flex_1().child(render_text_input(
                    app,
                    window,
                    cx,
                    "settings-git-user-name",
                    SettingsField::GitUserName,
                    "User Name",
                    "Jane Doe",
                    false,
                    false,
                    None,
                )))
                .child(div().flex_1().child(render_text_input(
                    app,
                    window,
                    cx,
                    "settings-git-user-email",
                    SettingsField::GitUserEmail,
                    "User Email",
                    "jane@example.com",
                    false,
                    false,
                    None,
                ))),
        )
        .child(render_text_input(
            app,
            window,
            cx,
            "settings-git-default-branch",
            SettingsField::GitDefaultBranch,
            "Default Branch",
            "main",
            false,
            false,
            Some("Used for new repositories created from this clone."),
        ))
        .child(
            div()
                .w_full()
                .p(theme::z(14.0))
                .rounded(theme::z(theme::CORNER_RADIUS))
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface_bg_muted())
                .child(
                    Switch::new("settings-pull-rebase")
                        .checked(app.repo.identity.pull_rebase.unwrap_or(false))
                        .label("Use pull.rebase")
                        .on_click(cx.listener(|app, checked: &bool, _window, cx| {
                            app.repo.identity.pull_rebase = Some(*checked);
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .mt(theme::z(8.0))
                        .text_size(theme::z(11.0))
                        .text_color(theme::text_muted())
                        .child(
                            "When enabled, `git pull` rebases instead of creating merge commits.",
                        ),
                ),
        )
}

fn render_ai_section(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(20.0))
        .child(render_section_header(
            "AI Commit",
            "Commit message generation",
            "These settings control the model and prompt used for AI commit suggestions.",
        ))
        .child(render_provider_group(app, cx))
        .child(render_model_group(app, window, cx))
        .child(render_endpoint_group(app))
        .child(render_text_input(
            app,
            window,
            cx,
            "settings-ai-api-key",
            SettingsField::AiApiKey,
            "API Key",
            app.settings.ai.provider.api_key_hint(),
            true,
            false,
            None,
        ))
        .child(render_text_input(
            app,
            window,
            cx,
            "settings-ai-system-prompt",
            SettingsField::AiSystemPrompt,
            "System Prompt",
            "Write a concise conventional commit message...",
            false,
            true,
            Some("Used verbatim when generating commit suggestions."),
        ))
}

fn render_updates_section(
    app: &GitSparkApp,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let current_channel = app.settings.update_channel;

    let status_element = match &app.update_status {
        UpdateStatus::Idle => div()
            .text_size(theme::z(12.0))
            .text_color(theme::text_muted())
            .child(format!("Current version: v{}", updater::CURRENT_VERSION))
            .into_any_element(),
        UpdateStatus::Checking => div()
            .text_size(theme::z(12.0))
            .text_color(theme::text_muted())
            .child("Checking for updates\u{2026}")
            .into_any_element(),
        UpdateStatus::Available(version) => h_flex()
            .gap(theme::z(8.0))
            .items_center()
            .child(
                div()
                    .text_size(theme::z(12.0))
                    .text_color(theme::accent())
                    .child(format!("Update available: v{version}")),
            )
            .child(
                render_secondary_button("settings-open-release", "View Release", cx)
                    .on_click(cx.listener(|app, _evt, _window, _cx| {
                        app.open_update_release_page();
                    })),
            )
            .into_any_element(),
        UpdateStatus::UpToDate => div()
            .text_size(theme::z(12.0))
            .text_color(theme::text_muted())
            .child(format!(
                "v{} is up to date.",
                updater::CURRENT_VERSION
            ))
            .into_any_element(),
        UpdateStatus::Error(err) => div()
            .text_size(theme::z(12.0))
            .text_color(theme::danger())
            .child(format!("Update check failed: {err}"))
            .into_any_element(),
        UpdateStatus::Downloading(version) => div()
            .text_size(theme::z(12.0))
            .text_color(theme::text_muted())
            .child(format!("Downloading v{version}\u{2026}"))
            .into_any_element(),
        UpdateStatus::ReadyToRestart(version) => div()
            .text_size(theme::z(12.0))
            .text_color(theme::accent())
            .child(format!("v{version} ready — restart to apply."))
            .into_any_element(),
    };

    let is_checking = app.update_status == UpdateStatus::Checking;

    v_flex()
        .w_full()
        .gap(theme::z(20.0))
        .child(render_section_header(
            "Updates",
            "Auto-update preferences",
            "Choose your update channel and check for new versions.",
        ))
        // Channel selector
        .child(
            v_flex()
                .w_full()
                .gap(theme::z(10.0))
                .child(render_field_label("Update Channel", None))
                .child(
                    h_flex()
                        .w_full()
                        .gap(theme::z(12.0))
                        .items_start()
                        .child(render_channel_radio(
                            app,
                            "settings-channel-release",
                            UpdateChannel::Release,
                            "Release",
                            "Stable releases only.",
                            cx,
                        ))
                        .child(render_channel_radio(
                            app,
                            "settings-channel-beta",
                            UpdateChannel::Beta,
                            "Beta",
                            "Preview of next stable. May contain rough edges.",
                            cx,
                        )),
                ),
        )
        // Version status
        .child(
            v_flex()
                .w_full()
                .gap(theme::z(10.0))
                .child(render_field_label("Version", None))
                .child(status_element),
        )
        // Check for Updates button
        .child(
            h_flex().child(
                render_secondary_button(
                    "settings-check-updates",
                    if is_checking {
                        "Checking\u{2026}"
                    } else {
                        "Check for Updates"
                    },
                    cx,
                )
                .disabled(is_checking)
                .on_click(cx.listener(|app, _evt, _window, cx| {
                    app.handle_settings_action(SettingsAction::CheckForUpdates, cx);
                })),
            ),
        )
}

fn render_channel_radio(
    app: &GitSparkApp,
    id: &'static str,
    channel: UpdateChannel,
    title: &'static str,
    description: &'static str,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let selected = app.settings.update_channel == channel;
    Radio::new(id)
        .checked(selected)
        .label(title)
        .on_click(cx.listener(move |app, _checked: &bool, _window, cx| {
            app.handle_settings_action(SettingsAction::ChangeUpdateChannel(channel), cx);
        }))
        .flex_1()
        .p(theme::z(12.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(if selected {
            theme::accent()
        } else {
            theme::border()
        })
        .bg(if selected {
            theme::surface_bg()
        } else {
            theme::surface_bg_muted()
        })
        .child(
            div()
                .text_size(theme::z(11.0))
                .text_color(theme::text_muted())
                .child(description),
        )
}

fn render_provider_group(app: &GitSparkApp, cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(10.0))
        .child(render_field_label("Provider", None))
        .child(
            h_flex()
                .w_full()
                .gap(theme::z(12.0))
                .items_start()
                .child(render_provider_radio(
                    app,
                    "settings-provider-openrouter",
                    AiProvider::OpenRouter,
                    "OpenRouter",
                    "Browse hosted models and keep the endpoint managed automatically.",
                    cx,
                ))
                .child(render_provider_radio(
                    app,
                    "settings-provider-openai-compatible",
                    AiProvider::OpenAICompatible,
                    "OpenAI Compatible",
                    "Use a direct OpenAI-compatible endpoint with a manual model name.",
                    cx,
                )),
        )
}

fn render_provider_radio(
    app: &GitSparkApp,
    id: &'static str,
    provider: AiProvider,
    title: &'static str,
    description: &'static str,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let selected = app.settings.ai.provider == provider;
    Radio::new(id)
        .checked(selected)
        .label(title)
        .on_click(cx.listener(move |app, _checked: &bool, window, cx| {
            app.handle_settings_action(SettingsAction::ChangeProvider(provider.clone()), cx);
            app.activate_settings_field(
                if provider == AiProvider::OpenRouter {
                    SettingsField::OpenRouterModelFilter
                } else {
                    SettingsField::AiModel
                },
                window,
                cx,
            );
        }))
        .flex_1()
        .p(theme::z(12.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(if selected {
            theme::accent()
        } else {
            theme::border()
        })
        .bg(if selected {
            theme::surface_bg()
        } else {
            theme::surface_bg_muted()
        })
        .child(
            div()
                .text_size(theme::z(11.0))
                .text_color(theme::text_muted())
                .child(description),
        )
}

fn render_model_group(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let provider = app.settings.ai.provider.clone();

    v_flex()
        .w_full()
        .gap(theme::z(10.0))
        .child(render_field_label("Model", None))
        .child(match provider {
            AiProvider::OpenRouter => render_openrouter_models(app, window, cx).into_any_element(),
            AiProvider::OpenAICompatible => render_text_input(
                app,
                window,
                cx,
                "settings-ai-model",
                SettingsField::AiModel,
                "Model",
                "gpt-4.1-mini",
                false,
                false,
                None,
            )
            .into_any_element(),
        })
}

fn render_openrouter_models(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let filter = app
        .filters
        .openrouter_model_filter
        .trim()
        .to_ascii_lowercase();
    let view = cx.entity().clone();

    let body: AnyElement = match &app.filters.openrouter_models {
        OpenRouterModelsState::Idle | OpenRouterModelsState::Loading => h_flex()
            .w_full()
            .items_center()
            .gap(theme::z(10.0))
            .p(theme::z(12.0))
            .child(Spinner::new().color(theme::text_muted()))
            .child(
                div()
                    .text_size(theme::z(12.0))
                    .text_color(theme::text_muted())
                    .child("Loading OpenRouter models..."),
            )
            .into_any_element(),
        OpenRouterModelsState::Error(message) => v_flex()
            .w_full()
            .gap(theme::z(12.0))
            .child(
                div()
                    .text_size(theme::z(12.0))
                    .text_color(theme::danger())
                    .child(message.clone()),
            )
            .child(
                render_primary_button("settings-openrouter-retry", "Retry", true, cx).on_click(
                    cx.listener(|app, _evt, _window, cx| {
                        app.handle_settings_action(SettingsAction::RetryOpenRouterModels, cx);
                    }),
                ),
            )
            .into_any_element(),
        OpenRouterModelsState::Ready(models) => {
            let filtered: Vec<RemoteModelOption> = models
                .iter()
                .filter(|model| {
                    filter.is_empty()
                        || model.id.to_ascii_lowercase().contains(&filter)
                        || model.name.to_ascii_lowercase().contains(&filter)
                })
                .cloned()
                .collect();
            let selected_model = app.settings.ai.model.clone();

            v_flex()
                .w_full()
                .gap(theme::z(10.0))
                .child(render_text_input(
                    app,
                    window,
                    cx,
                    "settings-openrouter-model-filter",
                    SettingsField::OpenRouterModelFilter,
                    "Search Models",
                    "Search models...",
                    false,
                    false,
                    None,
                ))
                .child(if filtered.is_empty() {
                    div()
                        .w_full()
                        .p(theme::z(12.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .border_1()
                        .border_color(theme::border())
                        .bg(theme::surface_bg_muted())
                        .child(
                            div()
                                .text_size(theme::z(11.0))
                                .text_color(theme::text_muted())
                                .child("No models match your search."),
                        )
                        .into_any_element()
                } else {
                    uniform_list(
                        "settings-openrouter-model-list",
                        filtered.len(),
                        move |range, _window, _cx| {
                            range
                                .map(|ix| {
                                    let model = filtered[ix].clone();
                                    render_model_option(
                                        &model,
                                        selected_model.as_str(),
                                        view.clone(),
                                    )
                                    .into_any_element()
                                })
                                .collect()
                        },
                    )
                    .with_sizing_behavior(ListSizingBehavior::Infer)
                    .h(theme::z(280.0))
                    .into_any_element()
                })
                .into_any_element()
        }
    };

    v_flex().w_full().gap(theme::z(8.0)).child(
        div()
            .w_full()
            .p(theme::z(14.0))
            .rounded(theme::z(theme::CORNER_RADIUS))
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface_bg_muted())
            .child(body),
    )
}

fn render_model_option(
    model: &RemoteModelOption,
    selected_model: &str,
    view: Entity<GitSparkApp>,
) -> impl IntoElement {
    let selected = model.id == selected_model;
    let model_id = model.id.clone();
    Radio::new(SharedString::from(format!("settings-model-{}", model.id)))
        .checked(selected)
        .label(truncate_single_line(&model.name, 48))
        .on_click(move |_checked: &bool, _window, cx| {
            let model_id = model_id.clone();
            view.update(cx, |app, cx| {
                app.handle_settings_action(SettingsAction::SelectOpenRouterModel(model_id), cx);
            });
        })
        .w_full()
        .p(theme::z(10.0))
        .rounded(theme::z(theme::CORNER_RADIUS_SM))
        .border_1()
        .border_color(if selected {
            theme::accent()
        } else {
            theme::border()
        })
        .bg(if selected {
            theme::surface_bg()
        } else {
            theme::panel_bg()
        })
        .child(
            div()
                .text_size(theme::z(11.0))
                .text_color(theme::text_muted())
                .truncate()
                .child(model.id.clone()),
        )
}

fn render_endpoint_group(app: &GitSparkApp) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(8.0))
        .child(render_field_label("Endpoint", None))
        .child(
            div()
                .w_full()
                .p(theme::z(12.0))
                .rounded(theme::z(theme::CORNER_RADIUS))
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface_bg_muted())
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_main())
                        .truncate()
                        .child(app.settings.ai.provider.default_endpoint()),
                ),
        )
}

fn render_section_header(eyebrow: &str, title: &str, description: &str) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(6.0))
        .child(
            div()
                .text_size(theme::z(10.0))
                .text_color(theme::text_muted())
                .font_weight(FontWeight::SEMIBOLD)
                .child(eyebrow.to_string()),
        )
        .child(
            div()
                .text_size(theme::z(20.0))
                .text_color(theme::text_main())
                .font_weight(FontWeight::BOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(theme::z(12.0))
                .text_color(theme::text_muted())
                .child(description.to_string()),
        )
}

fn render_field_label(label: &str, note: Option<&str>) -> impl IntoElement {
    let base = v_flex().gap(theme::z(4.0)).child(
        div()
            .text_size(theme::z(11.0))
            .text_color(theme::text_muted())
            .font_weight(FontWeight::SEMIBOLD)
            .child(label.to_string()),
    );

    if let Some(note) = note {
        base.child(
            div()
                .text_size(theme::z(10.0))
                .text_color(theme::text_muted())
                .child(note.to_string()),
        )
    } else {
        base
    }
}

#[allow(clippy::too_many_arguments)]
fn render_text_input(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
    id: &'static str,
    field: SettingsField,
    label: &str,
    placeholder: &str,
    password: bool,
    multiline: bool,
    note: Option<&str>,
) -> impl IntoElement {
    let value = app.settings_field_value(field);
    let cursor = app.settings_field_cursor(field).min(value.len());
    let focused = app.settings_field_focused(field, window);

    let display_value = if password && !focused {
        mask_password(value)
    } else {
        value.to_string()
    };

    let text = if display_value.is_empty() && !focused {
        div()
            .text_size(theme::z(12.0))
            .text_color(theme::text_muted())
            .child(placeholder.to_string())
    } else if focused {
        let before = display_value[..cursor].to_string();
        let after = display_value[cursor..].to_string();
        h_flex()
            .items_start()
            .w_full()
            .child(
                div()
                    .text_size(theme::z(12.0))
                    .text_color(theme::text_main())
                    .child(before),
            )
            .child(
                div()
                    .w(px(1.0))
                    .h(if multiline {
                        theme::z(16.0)
                    } else {
                        theme::z(14.0)
                    })
                    .bg(theme::text_main())
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .text_size(theme::z(12.0))
                    .text_color(theme::text_main())
                    .child(after),
            )
    } else {
        div()
            .text_size(theme::z(12.0))
            .text_color(theme::text_main())
            .child(display_value)
    };

    let text_container = if multiline {
        div().w_full()
    } else {
        div().w_full().truncate()
    };

    let field_shell = div()
        .id(id)
        .w_full()
        .min_h(if multiline {
            theme::z(160.0)
        } else {
            theme::z(42.0)
        })
        .p(theme::z(12.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .bg(theme::bg())
        .border_1()
        .border_color(if focused {
            theme::accent()
        } else {
            theme::border()
        })
        .cursor_text()
        .child(text_container.child(text))
        .on_click(cx.listener(move |app, _evt, window, cx| {
            app.activate_settings_field(field, window, cx);
        }));

    v_flex()
        .w_full()
        .gap(theme::z(8.0))
        .child(render_field_label(label, note))
        .child(field_shell)
}

fn render_primary_button(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    cx: &mut Context<GitSparkApp>,
) -> Button {
    Button::new(id)
        .label(label)
        .custom(
            ButtonCustomVariant::new(cx)
                .color(theme::commit_button_bg())
                .foreground(theme::commit_button_text())
                .hover(theme::commit_button_hover_bg())
                .active(theme::commit_button_hover_bg()),
        )
        .disabled(!enabled)
}

fn render_secondary_button(
    id: &'static str,
    label: &'static str,
    cx: &mut Context<GitSparkApp>,
) -> Button {
    Button::new(id).label(label).custom(
        ButtonCustomVariant::new(cx)
            .color(theme::surface_bg())
            .foreground(theme::text_main())
            .border(theme::border())
            .hover(theme::surface_bg_alt())
            .active(theme::surface_bg_alt()),
    )
}

fn mask_password(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "•".repeat(value.chars().count())
    }
}

fn truncate_single_line(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let mut chars = trimmed.chars();
    let shortened: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{shortened}…")
    } else {
        shortened
    }
}
