use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::divider::Divider;
use gpui_component::radio::Radio;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::spinner::Spinner;
use gpui_component::switch::Switch;
use gpui_component::{Disableable, Icon, IconName, h_flex, v_flex};

use crate::models::{AiProvider, RemoteModelOption};
use crate::ui::app::{GitSparkApp, SettingsAction};
use crate::ui::theme;
use crate::ui::ui_state::{OpenRouterModelsState, SettingsSection};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsField {
    GitUserName,
    GitUserEmail,
    GitDefaultBranch,
    AiModel,
    AiEndpoint,
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
    pub ai_endpoint_cursor: usize,
    pub ai_api_key_cursor: usize,
    pub ai_system_prompt_cursor: usize,
    pub openrouter_model_filter_cursor: usize,
    pub show_model_picker: bool,
    // Per-field selection anchors
    pub git_user_name_selection: Option<usize>,
    pub git_user_email_selection: Option<usize>,
    pub git_default_branch_selection: Option<usize>,
    pub ai_model_selection: Option<usize>,
    pub ai_endpoint_selection: Option<usize>,
    pub ai_api_key_selection: Option<usize>,
    pub ai_system_prompt_selection: Option<usize>,
    pub openrouter_model_filter_selection: Option<usize>,
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
            ai_endpoint_cursor: 0,
            ai_api_key_cursor: 0,
            ai_system_prompt_cursor: 0,
            openrouter_model_filter_cursor: 0,
            show_model_picker: false,
            git_user_name_selection: None,
            git_user_email_selection: None,
            git_default_branch_selection: None,
            ai_model_selection: None,
            ai_endpoint_selection: None,
            ai_api_key_selection: None,
            ai_system_prompt_selection: None,
            openrouter_model_filter_selection: None,
        }
    }
}

pub(crate) fn default_settings_field(section: SettingsSection) -> SettingsField {
    match section {
        SettingsSection::Git => SettingsField::GitUserName,
        SettingsSection::Ai => SettingsField::AiModel,
        SettingsSection::Appearance | SettingsSection::Integrations => SettingsField::GitUserName,
    }
}

pub(crate) fn render_settings_modal(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let bounds = window.bounds();
    let window_width = bounds.size.width / px(1.0);
    let window_height = bounds.size.height / px(1.0);
    let panel_width = (window_width - 32.0).clamp(720.0, 940.0);
    let panel_height = (window_height - 32.0).clamp(540.0, 720.0);
    let panel_left = ((window_width - panel_width) / 2.0).max(16.0);
    let panel_top = ((window_height - panel_height) / 2.0).max(16.0);

    let repo_scope = app
        .repo
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.repo.path.display().to_string());
    let status_text = if !app.messages.error_message.is_empty() {
        Some((app.messages.error_message.as_str(), theme::danger()))
    } else if !app.messages.status_message.is_empty() {
        Some((app.messages.status_message.as_str(), theme::text_muted()))
    } else {
        None
    };

    let section_action = match app.nav.settings_section {
        SettingsSection::Git => Some(
            render_primary_button("settings-save-git", "Save Git Config", true, cx)
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
        SettingsSection::Appearance | SettingsSection::Integrations => None,
    };

    let content = match app.nav.settings_section {
        SettingsSection::Git => {
            render_git_section(app, window, repo_scope.as_deref(), cx).into_any_element()
        }
        SettingsSection::Ai => render_ai_section(app, window, cx).into_any_element(),
        SettingsSection::Appearance => render_appearance_section().into_any_element(),
        SettingsSection::Integrations => render_integrations_section().into_any_element(),
    };

    let lock_content_scroll = app.nav.settings_section == SettingsSection::Ai
        && app.settings.ai.provider == AiProvider::OpenRouter
        && app.settings_modal.show_model_picker;

    let content_body = v_flex()
        .w_full()
        .items_center()
        .px(theme::z(24.0))
        .py(theme::z(14.0))
        .child(div().w_full().max_w(theme::z(680.0)).child(content));

    let content_scroll: AnyElement = {
        let base = div()
            .id("settings-content-scroll")
            .size_full()
            .bg(theme::panel_bg());
        if lock_content_scroll {
            base.overflow_hidden()
                .child(content_body)
                .into_any_element()
        } else {
            base.overflow_y_scrollbar()
                .child(content_body)
                .into_any_element()
        }
    };

    let panel = v_flex()
        .id("settings-modal-panel")
        .track_focus(&app.settings_modal.focus)
        .key_context("settings-modal")
        .occlude()
        .absolute()
        .left(px(panel_left))
        .top(px(panel_top))
        .w(px(panel_width))
        .h(px(panel_height))
        .bg(theme::panel_bg())
        .border_1()
        .border_color(theme::border())
        .rounded(theme::z(theme::CORNER_RADIUS))
        .shadow_lg()
        .overflow_hidden()
        .child(render_header(cx))
        .child(
            h_flex()
                .flex_1()
                .w_full()
                .overflow_hidden()
                .child(render_nav(app, cx))
                .child(Divider::vertical().color(theme::border()))
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .overflow_hidden()
                        .child(content_scroll),
                ),
        )
        .child(Divider::horizontal().color(theme::border()))
        .child(
            h_flex()
                .w_full()
                .min_h(theme::z(60.0))
                .px(theme::z(24.0))
                .py(theme::z(12.0))
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
        .h(theme::z(50.0))
        .px(theme::z(16.0))
        .justify_between()
        .items_center()
        .gap(theme::z(16.0))
        .border_b_1()
        .border_color(theme::border())
        .child(
            div()
                .text_size(theme::z(14.0))
                .text_color(theme::text_main())
                .font_weight(FontWeight::BOLD)
                .child("Settings"),
        )
        .child(
            div()
                .id("settings-close-header")
                .w(theme::z(28.0))
                .h(theme::z(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme::z(theme::CORNER_RADIUS_SM))
                .text_size(theme::z(18.0))
                .text_color(theme::text_muted())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover_bg()).text_color(theme::text_main()))
                .child("\u{00d7}")
                .on_click(cx.listener(|app, _evt, _window, cx| {
                    app.handle_settings_action(SettingsAction::Close, cx);
                })),
        )
}

fn render_nav(app: &GitSparkApp, cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    let sections = [
        (
            SettingsSection::Git,
            "settings-tab-git",
            "Git",
            IconName::Settings2,
        ),
        (
            SettingsSection::Ai,
            "settings-tab-ai",
            "AI Commit",
            IconName::Bot,
        ),
        (
            SettingsSection::Appearance,
            "settings-tab-appearance",
            "Appearance",
            IconName::Palette,
        ),
        (
            SettingsSection::Integrations,
            "settings-tab-integrations",
            "Integrations",
            IconName::SquareTerminal,
        ),
    ];

    let mut rail = v_flex()
        .id("settings-nav")
        .w(theme::z(200.0))
        .h_full()
        .flex_shrink_0()
        .p(theme::z(14.0))
        .gap(theme::z(6.0))
        .bg(theme::surface_bg_muted());

    for (section, test_id, label, icon) in sections {
        let is_active = app.nav.settings_section == section;
        rail = rail.child(
            h_flex()
                .id(test_id)
                .w_full()
                .h(theme::z(38.0))
                .px(theme::z(12.0))
                .gap(theme::z(10.0))
                .items_center()
                .rounded(theme::z(theme::CORNER_RADIUS))
                .cursor_pointer()
                .bg(if is_active {
                    theme::commit_button_bg()
                } else {
                    gpui::transparent_black().into()
                })
                .hover(move |s| {
                    s.bg(if is_active {
                        theme::commit_button_bg()
                    } else {
                        theme::hover_bg()
                    })
                })
                .child(
                    Icon::new(icon)
                        .size(theme::z(14.0))
                        .text_color(if is_active {
                            theme::commit_button_text()
                        } else {
                            theme::text_muted()
                        }),
                )
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(if is_active {
                            theme::commit_button_text()
                        } else {
                            theme::text_main()
                        })
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(label),
                )
                .on_click(cx.listener(move |app, _evt, window, cx| {
                    app.nav.settings_section = section;
                    let field = if section == SettingsSection::Ai
                        && app.settings.ai.provider == AiProvider::OpenRouter
                    {
                        SettingsField::OpenRouterModelFilter
                    } else {
                        default_settings_field(section)
                    };
                    app.activate_settings_field(field, window, cx);
                })),
        );
    }

    rail
}

fn render_git_section(
    app: &GitSparkApp,
    window: &Window,
    repo_scope: Option<&str>,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let has_repo = repo_scope.is_some();
    let description = repo_scope
        .map(|path| {
            format!("Author, default branch, and pull behavior apply to this repository: {path}.")
        })
        .unwrap_or_else(|| {
            "Author and default branch are stored in global Git config.".to_string()
        });

    v_flex()
        .w_full()
        .gap(theme::z(20.0))
        .child(render_section_header(
            "Git",
            "Git configuration",
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
            Some("Used as the default branch name for new repositories."),
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
                    h_flex()
                        .gap(theme::z(10.0))
                        .items_center()
                        .child(
                            Switch::new("settings-pull-rebase")
                                .checked(app.repo.identity.pull_rebase.unwrap_or(false))
                                .disabled(!has_repo)
                                .on_click(cx.listener(|app, checked: &bool, _window, cx| {
                                    app.repo.identity.pull_rebase = Some(*checked);
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(if has_repo {
                                    theme::text_main()
                                } else {
                                    theme::text_muted()
                                })
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Use pull.rebase"),
                        ),
                )
                .child(
                    div()
                        .mt(theme::z(8.0))
                        .text_size(theme::z(11.0))
                        .text_color(theme::text_muted())
                        .child(if has_repo {
                            "When enabled, `git pull` rebases instead of creating merge commits."
                        } else {
                            "Open a repository to configure pull behavior for that repository."
                        }),
                ),
        )
}

fn render_appearance_section() -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(22.0))
        .child(render_section_header(
            "Appearance",
            "Theme",
            "GitSpark uses the GitHub Desktop dark palette for the native macOS interface.",
        ))
        .child(
            v_flex()
                .w_full()
                .gap(theme::z(10.0))
                .child(render_field_label("Theme", None))
                .child(
                    h_flex()
                        .w_full()
                        .gap(theme::z(12.0))
                        .items_start()
                        .child(render_theme_option(
                            "settings-theme-light",
                            "Light",
                            false,
                            false,
                        ))
                        .child(render_theme_option(
                            "settings-theme-dark",
                            "Dark",
                            true,
                            true,
                        )),
                )
                .child(render_theme_option(
                    "settings-theme-system",
                    "System",
                    false,
                    false,
                )),
        )
        .child(
            v_flex()
                .w_full()
                .gap(theme::z(8.0))
                .child(render_field_label("Diff", None))
                .child(render_static_dropdown(
                    "settings-tab-size",
                    "Tab Size",
                    "4 (default)",
                    Some("Used by the diff viewer."),
                )),
        )
}

fn render_theme_option(
    id: &'static str,
    label: &'static str,
    dark_preview: bool,
    selected: bool,
) -> impl IntoElement {
    let border = if selected {
        theme::accent()
    } else {
        theme::border()
    };
    let shell_bg = if dark_preview {
        theme::surface_bg()
    } else {
        gpui::rgb(0xf6f8fa).into()
    };
    let sidebar_bg = if dark_preview {
        theme::bg()
    } else {
        gpui::rgb(0xffffff).into()
    };
    v_flex()
        .id(id)
        .flex_1()
        .max_w(theme::z(274.0))
        .min_h(theme::z(126.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(border)
        .bg(theme::surface_bg_muted())
        .overflow_hidden()
        .child(
            v_flex()
                .h(theme::z(84.0))
                .bg(shell_bg)
                .border_b_1()
                .border_color(theme::border())
                .child(
                    h_flex()
                        .h(theme::z(20.0))
                        .px(theme::z(8.0))
                        .gap(theme::z(5.0))
                        .items_center()
                        .child(theme_preview_dot(theme::text_muted()))
                        .child(theme_preview_bar(theme::z(28.0), theme::text_muted()))
                        .child(theme_preview_dot(theme::text_muted()))
                        .child(theme_preview_bar(theme::z(34.0), theme::text_muted())),
                )
                .child(
                    h_flex()
                        .flex_1()
                        .child(
                            v_flex()
                                .w(theme::z(58.0))
                                .h_full()
                                .p(theme::z(6.0))
                                .gap(theme::z(5.0))
                                .bg(sidebar_bg)
                                .child(theme_preview_bar(theme::z(38.0), theme::text_muted()))
                                .child(theme_preview_bar(theme::z(32.0), theme::text_muted()))
                                .child(theme_preview_bar(
                                    theme::z(42.0),
                                    theme::commit_button_bg(),
                                )),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .h_full()
                                .p(theme::z(8.0))
                                .gap(theme::z(5.0))
                                .child(theme_preview_bar(theme::z(72.0), theme::success()))
                                .child(theme_preview_bar(theme::z(48.0), theme::danger()))
                                .child(theme_preview_bar(theme::z(60.0), theme::danger())),
                        ),
                ),
        )
        .child(
            h_flex()
                .h(theme::z(40.0))
                .px(theme::z(12.0))
                .gap(theme::z(8.0))
                .items_center()
                .child(
                    div()
                        .w(theme::z(14.0))
                        .h(theme::z(14.0))
                        .rounded_full()
                        .border_1()
                        .border_color(if selected {
                            theme::accent()
                        } else {
                            theme::text_muted()
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(selected, |el| {
                            el.child(
                                div()
                                    .w(theme::z(7.0))
                                    .h(theme::z(7.0))
                                    .rounded_full()
                                    .bg(theme::accent()),
                            )
                        }),
                )
                .child(
                    div()
                        .text_size(theme::z(13.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(label),
                ),
        )
}

fn theme_preview_dot(color: Hsla) -> impl IntoElement {
    div()
        .w(theme::z(4.0))
        .h(theme::z(4.0))
        .rounded_full()
        .bg(theme::with_alpha(color, 0.7))
}

fn theme_preview_bar(width: Pixels, color: Hsla) -> impl IntoElement {
    div()
        .w(width)
        .h(theme::z(4.0))
        .rounded(theme::z(2.0))
        .bg(theme::with_alpha(color, 0.78))
}

fn render_integrations_section() -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(22.0))
        .child(render_section_header(
            "Integrations",
            "External tools",
            "GitSpark opens files with your Git editor or the macOS default application.",
        ))
        .child(
            v_flex()
                .w_full()
                .gap(theme::z(16.0))
                .child(render_static_dropdown(
                    "settings-external-editor",
                    "External Editor",
                    "Git core.editor, VISUAL, EDITOR, then default app",
                    Some("Used by Open in External Editor."),
                ))
                .child(render_static_dropdown(
                    "settings-shell",
                    "Shell",
                    "macOS Terminal",
                    Some("Used for shell-based editor commands."),
                )),
        )
}

fn render_static_dropdown(
    id: &'static str,
    label: &'static str,
    value: &'static str,
    note: Option<&str>,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(8.0))
        .child(render_field_label(label, note))
        .child(
            h_flex()
                .id(id)
                .w_full()
                .h(theme::z(34.0))
                .px(theme::z(10.0))
                .gap(theme::z(8.0))
                .items_center()
                .justify_between()
                .rounded(theme::z(theme::CORNER_RADIUS))
                .border_1()
                .border_color(theme::border())
                .bg(theme::bg())
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_size(theme::z(13.0))
                        .text_color(theme::text_main())
                        .child(value),
                )
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(theme::z(12.0))
                        .text_color(theme::text_muted()),
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
        .gap(theme::z(10.0))
        .child(render_section_header(
            "AI Commit",
            "Commit message generation",
            "Choose the provider, model, endpoint, and prompt used for AI commit suggestions.",
        ))
        .child(render_provider_group(app, cx))
        .child(render_model_group(app, window, cx))
        .child(render_endpoint_group(app, window, cx))
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

fn render_provider_group(app: &GitSparkApp, cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(8.0))
        .child(render_field_label("Provider", None))
        .child(
            h_flex()
                .w_full()
                .gap(theme::z(10.0))
                .child(div().flex_1().min_w_0().child(render_provider_radio(
                    app,
                    "settings-provider-openrouter",
                    AiProvider::OpenRouter,
                    "OpenRouter",
                    "Browse hosted models and keep the endpoint managed automatically.",
                    cx,
                )))
                .child(div().flex_1().min_w_0().child(render_provider_radio(
                    app,
                    "settings-provider-openai-compatible",
                    AiProvider::OpenAICompatible,
                    "OpenAI Compatible",
                    "Use a direct OpenAI-compatible endpoint with a manual model name.",
                    cx,
                ))),
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
        .w_full()
        .min_h(theme::z(58.0))
        .p(theme::z(9.0))
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

    match provider {
        AiProvider::OpenRouter => v_flex()
            .w_full()
            .gap(theme::z(10.0))
            .child(render_field_label("Model", None))
            .child(render_openrouter_models(app, window, cx))
            .into_any_element(),
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
    }
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
            let selected_model = app.settings.ai.model.clone();
            let current_model_name = models
                .iter()
                .find(|m| m.id == selected_model)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| selected_model.clone());

            if !app.settings_modal.show_model_picker {
                // Collapsed: show current model as a clickable field
                let vh = cx.entity().clone();
                h_flex()
                    .id("model-picker-collapsed")
                    .w_full()
                    .h(theme::z(32.0))
                    .px(theme::z(10.0))
                    .items_center()
                    .justify_between()
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::bg())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::list_hover_bg()))
                    .child(
                        div()
                            .flex_1()
                            .text_size(theme::z(13.0))
                            .text_color(theme::text_main())
                            .truncate()
                            .child(current_model_name),
                    )
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size(theme::z(12.0))
                            .text_color(theme::text_muted()),
                    )
                    .on_click(move |_evt, _win, cx| {
                        vh.update(cx, |app, cx| {
                            app.settings_modal.show_model_picker = true;
                            cx.notify();
                        });
                    })
                    .into_any_element()
            } else {
                // Expanded: search + model list
                let vh_close = cx.entity().clone();
                let filtered: Vec<RemoteModelOption> = models
                    .iter()
                    .filter(|model| {
                        filter.is_empty()
                            || model.id.to_ascii_lowercase().contains(&filter)
                            || model.name.to_ascii_lowercase().contains(&filter)
                    })
                    .cloned()
                    .collect();

                v_flex()
                    .id("model-picker-expanded")
                    .w_full()
                    .gap(theme::z(6.0))
                    .on_scroll_wheel(|_event, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_mouse_down_out(move |_evt, _win, cx| {
                        vh_close.update(cx, |app, cx| {
                            app.settings_modal.show_model_picker = false;
                            cx.notify();
                        });
                    })
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
                        .h(theme::z(220.0))
                        .into_any_element()
                    })
                    .into_any_element()
            }
        }
    };

    v_flex()
        .w_full()
        .gap(theme::z(8.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_bg_muted())
        .p(theme::z(8.0))
        .child(body)
}

fn render_model_option(
    model: &RemoteModelOption,
    selected_model: &str,
    view: Entity<GitSparkApp>,
) -> impl IntoElement {
    let selected = model.id == selected_model;
    let model_id = model.id.clone();

    h_flex()
        .id(SharedString::from(format!("settings-model-{}", model.id)))
        .w_full()
        .px(theme::z(10.0))
        .py(theme::z(6.0))
        .gap(theme::z(8.0))
        .items_center()
        .cursor_pointer()
        .rounded(theme::z(theme::CORNER_RADIUS_SM))
        .bg(if selected {
            theme::accent()
        } else {
            gpui::transparent_black()
        })
        .hover(move |s| {
            s.bg(if selected {
                theme::accent()
            } else {
                theme::list_hover_bg()
            })
        })
        .on_click(move |_evt, _win, cx| {
            let model_id = model_id.clone();
            view.update(cx, |app, cx| {
                app.handle_settings_action(SettingsAction::SelectOpenRouterModel(model_id), cx);
                app.settings_modal.show_model_picker = false;
            });
        })
        // Radio dot
        .child(
            div()
                .w(theme::z(14.0))
                .h(theme::z(14.0))
                .rounded_full()
                .border_1()
                .border_color(if selected {
                    gpui::white()
                } else {
                    theme::text_muted()
                })
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .when(selected, |el| {
                    el.child(
                        div()
                            .w(theme::z(8.0))
                            .h(theme::z(8.0))
                            .rounded_full()
                            .bg(gpui::white()),
                    )
                }),
        )
        // Model name + id
        .child(
            v_flex()
                .flex_1()
                .overflow_hidden()
                .child(
                    div()
                        .text_size(theme::z(13.0))
                        .text_color(if selected {
                            gpui::white().into()
                        } else {
                            theme::text_main()
                        })
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(model.name.clone()),
                )
                .child(
                    div()
                        .text_size(theme::z(11.0))
                        .text_color(if selected {
                            theme::with_alpha(gpui::white().into(), 0.7)
                        } else {
                            theme::text_muted()
                        })
                        .truncate()
                        .child(model.id.clone()),
                ),
        )
}

fn render_endpoint_group(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    if app.settings.ai.provider == AiProvider::OpenRouter {
        v_flex()
            .w_full()
            .gap(theme::z(8.0))
            .child(render_field_label(
                "Endpoint",
                Some("Managed automatically for OpenRouter."),
            ))
            .child(
                div()
                    .id("settings-ai-endpoint")
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
            .into_any_element()
    } else {
        render_text_input(
            app,
            window,
            cx,
            "settings-ai-endpoint",
            SettingsField::AiEndpoint,
            "Endpoint",
            AiProvider::OpenAICompatible.default_endpoint(),
            false,
            false,
            Some("Use any OpenAI-compatible chat completions endpoint."),
        )
        .into_any_element()
    }
}

fn render_section_header(eyebrow: &str, title: &str, description: &str) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(5.0))
        .child(
            div()
                .text_size(theme::z(10.0))
                .text_color(theme::text_muted())
                .font_weight(FontWeight::SEMIBOLD)
                .child(eyebrow.to_string()),
        )
        .child(
            div()
                .text_size(theme::z(17.0))
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
    use crate::ui::text_field;

    let value = app.settings_field_value(field);
    let cursor = app.settings_field_cursor(field).min(value.len());
    let selection = app.settings_field_selection(field);
    let focused = app.settings_field_focused(field, window);

    let display_value = if password && !focused {
        mask_password(value)
    } else {
        value.to_string()
    };

    let text = text_field::render_text_content(
        &display_value,
        cursor,
        selection,
        focused,
        placeholder,
        multiline,
    );

    let field_shell = div()
        .id(id)
        .track_focus(&app.settings_modal.focus)
        .key_context("text-field")
        .on_key_down(cx.listener(GitSparkApp::handle_settings_key))
        .w_full()
        .min_h(if multiline {
            theme::z(80.0)
        } else {
            theme::z(36.0)
        })
        .px(theme::z(12.0))
        .py(theme::z(8.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .bg(theme::bg())
        .border_1()
        .border_color(if focused {
            theme::accent()
        } else {
            theme::border()
        })
        .cursor_text()
        .child(text)
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

fn mask_password(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "•".repeat(value.chars().count())
    }
}

#[allow(dead_code)]
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
