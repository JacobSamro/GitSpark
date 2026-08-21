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
use crate::ui::app::{GitSparkApp, SettingsAction, ToolbarAction};
use crate::ui::ids::stable_id_slug;
use crate::ui::theme;
use crate::ui::ui_state::{OpenRouterModelsState, SettingsSection};

const SETTINGS_MODAL_MARGIN: f32 = 16.0;
const SETTINGS_MODAL_MIN_WIDTH: f32 = 720.0;
const SETTINGS_MODAL_MAX_WIDTH: f32 = 940.0;
const SETTINGS_MODAL_MIN_HEIGHT: f32 = 540.0;
const SETTINGS_MODAL_MAX_HEIGHT: f32 = 760.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsField {
    RemoteUrl,
    IgnoredFiles,
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
    pub remote_url_cursor: usize,
    pub ignored_files_cursor: usize,
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
    pub remote_url_selection: Option<usize>,
    pub ignored_files_selection: Option<usize>,
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
            remote_url_cursor: 0,
            ignored_files_cursor: 0,
            git_user_name_cursor: 0,
            git_user_email_cursor: 0,
            git_default_branch_cursor: 0,
            ai_model_cursor: 0,
            ai_endpoint_cursor: 0,
            ai_api_key_cursor: 0,
            ai_system_prompt_cursor: 0,
            openrouter_model_filter_cursor: 0,
            show_model_picker: false,
            remote_url_selection: None,
            ignored_files_selection: None,
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
        SettingsSection::Remote => SettingsField::RemoteUrl,
        SettingsSection::IgnoredFiles => SettingsField::IgnoredFiles,
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
    let (panel_width, panel_height, panel_left, panel_top) =
        settings_modal_geometry(window_width, window_height);

    let repo_scope = app
        .settings_has_repository_scope()
        .then(|| {
            app.repo
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.repo.path.display().to_string())
        })
        .flatten();
    let status_text = settings_status_text(app);

    let section_action = match app.nav.settings_section {
        SettingsSection::Remote => app.repo.remote_name.as_ref().map(|_| {
            render_primary_button("settings-save-remote", "Save Remote", true, cx)
                .on_click(cx.listener(|app, _evt, _window, cx| {
                    app.handle_settings_action(SettingsAction::SaveRemote, cx);
                }))
                .into_any_element()
        }),
        SettingsSection::IgnoredFiles => Some(
            render_primary_button("settings-save-ignored-files", "Save", true, cx)
                .on_click(cx.listener(|app, _evt, _window, cx| {
                    app.handle_settings_action(SettingsAction::SaveIgnoredFiles, cx);
                }))
                .into_any_element(),
        ),
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
        SettingsSection::Remote => render_remote_section(app, window, cx).into_any_element(),
        SettingsSection::IgnoredFiles => {
            render_ignored_files_section(app, window, cx).into_any_element()
        }
        SettingsSection::Git => {
            render_git_section(app, window, repo_scope.as_deref(), cx).into_any_element()
        }
        SettingsSection::Ai => render_ai_section(app, window, cx).into_any_element(),
        SettingsSection::Appearance => render_appearance_section(cx).into_any_element(),
        SettingsSection::Integrations => render_integrations_section().into_any_element(),
    };

    let content_body = v_flex()
        .w_full()
        .items_center()
        .px(theme::z(24.0))
        .py(theme::z(12.0))
        .child(div().w_full().max_w(theme::z(680.0)).child(content));

    let content_scroll = div()
        .id("settings-content-scroll")
        .size_full()
        .bg(theme::panel_bg())
        .overflow_y_scrollbar()
        .child(content_body);

    let panel = v_flex()
        .id("settings-modal-panel")
        .track_focus(&app.settings_modal.focus)
        .key_context("settings-modal")
        .occlude()
        .w(px(panel_width))
        .h(px(panel_height))
        .bg(theme::panel_bg())
        .border_1()
        .border_color(theme::border())
        .rounded(theme::z(theme::CORNER_RADIUS))
        .shadow_lg()
        .overflow_hidden()
        .child(render_header(app, cx))
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
                .min_h(theme::z(52.0))
                .px(theme::z(24.0))
                .py(theme::z(8.0))
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
        .child(
            div()
                .id("settings-modal-container")
                .absolute()
                .left(px(panel_left))
                .top(px(panel_top))
                .on_click(|_evt, _window, cx| cx.stop_propagation())
                .child(panel),
        )
}

fn settings_modal_geometry(window_width: f32, window_height: f32) -> (f32, f32, f32, f32) {
    let available_width = (window_width - (SETTINGS_MODAL_MARGIN * 2.0)).max(0.0);
    let available_height = (window_height - (SETTINGS_MODAL_MARGIN * 2.0)).max(0.0);
    let panel_width = available_width
        .min(SETTINGS_MODAL_MAX_WIDTH)
        .max(SETTINGS_MODAL_MIN_WIDTH.min(available_width));
    let panel_height = available_height
        .min(SETTINGS_MODAL_MAX_HEIGHT)
        .max(SETTINGS_MODAL_MIN_HEIGHT.min(available_height));
    let panel_left = ((window_width - panel_width) / 2.0).max(SETTINGS_MODAL_MARGIN);
    let panel_top = ((window_height - panel_height) / 2.0).max(SETTINGS_MODAL_MARGIN);

    (panel_width, panel_height, panel_left, panel_top)
}

fn render_header(app: &GitSparkApp, cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    let title = if app.settings_has_repository_scope() {
        "Repository Settings"
    } else {
        "Global Settings"
    };

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
                .child(title),
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

fn settings_status_text(app: &GitSparkApp) -> Option<(&str, Hsla)> {
    if !app.messages.error_message.is_empty() {
        return Some((app.messages.error_message.as_str(), theme::danger()));
    }

    match app.messages.status_message.as_str() {
        "AI settings saved."
        | "Git config saved."
        | "Remote settings saved."
        | "Ignored files saved." => {
            Some((app.messages.status_message.as_str(), theme::text_muted()))
        }
        _ => None,
    }
}

fn render_nav(app: &GitSparkApp, cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    let sections = if app.settings_has_repository_scope() {
        vec![
            (
                SettingsSection::Remote,
                "settings-tab-remote",
                "Remote",
                IconName::GitHub,
            ),
            (
                SettingsSection::IgnoredFiles,
                "settings-tab-ignored-files",
                "Ignored Files",
                IconName::FolderClosed,
            ),
            (
                SettingsSection::Git,
                "settings-tab-git",
                "Git",
                IconName::Settings2,
            ),
        ]
    } else {
        vec![
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
        ]
    };

    let mut rail = v_flex()
        .id("settings-nav")
        .w(theme::z(200.0))
        .h_full()
        .flex_shrink_0()
        .p(theme::z(14.0))
        .gap(theme::z(6.0))
        .bg(theme::bg());

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
                    app.nav.settings_section = app
                        .nav
                        .settings_scope
                        .normalize_section(app.nav.settings_section);
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

fn render_remote_section(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let Some(remote_name) = app.repo.remote_name.as_deref() else {
        return v_flex()
            .w_full()
            .gap(theme::z(18.0))
            .child(render_section_header(
                "Remote",
                "No remote configured",
                "Publish this repository to GitHub or add a remote from the command line.",
            ))
            .child(
                div()
                    .id("settings-remote-empty")
                    .w_full()
                    .p(theme::z(14.0))
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::bg())
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap(theme::z(12.0))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(theme::z(12.0))
                                    .text_color(theme::text_muted())
                                    .child("This repository does not have a remote URL yet."),
                            )
                            .child(
                                render_primary_button(
                                    "settings-remote-publish",
                                    "Publish Repository",
                                    true,
                                    cx,
                                )
                                .on_click(cx.listener(|app, _evt, _window, cx| {
                                    app.handle_toolbar_action(
                                        ToolbarAction::RunNetworkAction(
                                            crate::ui::domain_state::NetworkAction::PublishRepository,
                                        ),
                                        cx,
                                    );
                                })),
                            ),
                    ),
            )
            .into_any_element();
    };

    v_flex()
        .w_full()
        .gap(theme::z(18.0))
        .child(render_section_header(
            "Remote",
            "Primary remote repository",
            &format!("Edit the URL used for the `{remote_name}` remote."),
        ))
        .child(render_text_input(
            app,
            window,
            cx,
            "settings-remote-url",
            SettingsField::RemoteUrl,
            &format!("Primary Remote Repository ({remote_name}) URL"),
            "Remote URL",
            false,
            false,
            false,
            Some("Used by fetch, pull, push, and GitHub links."),
        ))
        .into_any_element()
}

fn render_ignored_files_section(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(18.0))
        .child(render_section_header(
            "Ignored Files",
            "Repository .gitignore",
            "Edit patterns for intentionally untracked files in this repository.",
        ))
        .child(
            div()
                .text_size(theme::z(12.0))
                .text_color(theme::text_muted())
                .line_height(theme::z(18.0))
                .child(
                    "Files already tracked by Git are not affected. Leave this empty to remove the root .gitignore file.",
                ),
        )
        .child(render_text_input(
            app,
            window,
            cx,
            "settings-ignored-files-text",
            SettingsField::IgnoredFiles,
            "Ignored files",
            "Ignored files",
            false,
            true,
            false,
            Some("One pattern per line."),
        ))
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
            format!("Choose the author identity used for commits in this repository: {path}.")
        })
        .unwrap_or_else(|| {
            "Author identity and repository defaults are stored in global Git config.".to_string()
        });
    let inherited_global_identity = has_repo && !app.repo.use_local_identity;
    let title = if has_repo {
        "Repository Git identity"
    } else {
        "Global Git configuration"
    };

    v_flex()
        .w_full()
        .gap(theme::z(18.0))
        .child(render_section_header("Git", title, &description))
        .children(if has_repo {
            Some(render_git_config_scope(app, cx).into_any_element())
        } else {
            None
        })
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
                    inherited_global_identity,
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
                    inherited_global_identity,
                    None,
                ))),
        )
        .child(render_git_defaults_section(
            app,
            window,
            has_repo,
            inherited_global_identity,
            cx,
        ))
}

fn render_git_defaults_section(
    app: &GitSparkApp,
    window: &Window,
    has_repo: bool,
    inherited_global_identity: bool,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let default_branch = render_text_input(
        app,
        window,
        cx,
        "settings-git-default-branch",
        SettingsField::GitDefaultBranch,
        "Default Branch",
        "main",
        false,
        false,
        inherited_global_identity,
        None,
    );

    let default_branch_row = if has_repo {
        h_flex()
            .w_full()
            .gap(theme::z(14.0))
            .items_start()
            .child(div().flex_1().min_w_0().child(default_branch))
            .child(
                v_flex()
                    .id("settings-pull-rebase-card")
                    .flex_1()
                    .min_w_0()
                    .gap(theme::z(6.0))
                    .child(
                        div()
                            .text_size(theme::z(11.0))
                            .text_color(theme::text_muted())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Pull Behavior"),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_h(theme::z(36.0))
                            .px(theme::z(12.0))
                            .py(theme::z(8.0))
                            .rounded(theme::z(theme::CORNER_RADIUS))
                            .border_1()
                            .border_color(theme::border())
                            .bg(theme::bg())
                            .child(
                                h_flex()
                                    .gap(theme::z(10.0))
                                    .items_center()
                                    .child(
                                        Switch::new("settings-pull-rebase")
                                            .checked(app.repo.identity.pull_rebase.unwrap_or(false))
                                            .on_click(cx.listener(
                                                |app, checked: &bool, _window, cx| {
                                                    app.repo.identity.pull_rebase = Some(*checked);
                                                    cx.notify();
                                                },
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_main())
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Use pull.rebase"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .text_size(theme::z(11.0))
                            .text_color(theme::text_muted())
                            .child("Rebase when pulling instead of creating merge commits."),
                    ),
            )
            .into_any_element()
    } else {
        div()
            .w_full()
            .max_w(theme::z(424.0))
            .child(default_branch)
            .into_any_element()
    };

    v_flex()
        .w_full()
        .gap(theme::z(8.0))
        .pt(theme::z(2.0))
        .child(render_section_subhead(
            if has_repo {
                "Defaults and pull behavior"
            } else {
                "Default branch"
            },
            if has_repo {
                "These settings are separate from the author identity above."
            } else {
                "Used for new repositories created from this global Git configuration."
            },
        ))
        .child(default_branch_row)
}

fn render_git_config_scope(app: &GitSparkApp, cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(8.0))
        .child(render_field_label("For this repository", None))
        .child(
            h_flex()
                .w_full()
                .gap(theme::z(10.0))
                .child(div().flex_1().min_w_0().child(render_git_scope_radio(
                    app,
                    "settings-git-scope-global",
                    false,
                    "Use my global Git config",
                    "Clear local author overrides and use your global name and email.",
                    cx,
                )))
                .child(div().flex_1().min_w_0().child(render_git_scope_radio(
                    app,
                    "settings-git-scope-local",
                    true,
                    "Use a local Git config",
                    "Store a name and email only for this repository.",
                    cx,
                ))),
        )
}

fn render_git_scope_radio(
    app: &GitSparkApp,
    id: &'static str,
    use_local: bool,
    title: &'static str,
    description: &'static str,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let selected = app.repo.use_local_identity == use_local;
    let radio_id = if use_local {
        "settings-git-scope-local-radio"
    } else {
        "settings-git-scope-global-radio"
    };
    let radio = Radio::new(radio_id)
        .checked(selected)
        .label(title)
        .on_click(cx.listener(move |app, _checked: &bool, window, cx| {
            app.handle_settings_action(SettingsAction::SetGitConfigScope(use_local), cx);
            app.activate_settings_field(SettingsField::GitUserName, window, cx);
        }));

    div()
        .id(id)
        .w_full()
        .p(theme::z(10.0))
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
            theme::bg()
        })
        .cursor_pointer()
        .hover(|style| style.bg(theme::hover_bg()))
        .on_click(cx.listener(move |app, _evt, window, cx| {
            app.handle_settings_action(SettingsAction::SetGitConfigScope(use_local), cx);
            if use_local {
                app.activate_settings_field(SettingsField::GitUserName, window, cx);
            }
            cx.stop_propagation();
        }))
        .child(
            v_flex().gap(theme::z(4.0)).child(radio).child(
                div()
                    .text_size(theme::z(11.0))
                    .text_color(theme::text_muted())
                    .child(description),
            ),
        )
}

fn render_appearance_section(cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    let current = theme::appearance();

    v_flex()
        .w_full()
        .gap(theme::z(22.0))
        .child(render_section_header(
            "Appearance",
            "Theme",
            "Light and dark are the same tokens resolved differently — see design.md \u{00A7}13.",
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
                            current == theme::Appearance::Light,
                            theme::Appearance::Light,
                            cx,
                        ))
                        .child(render_theme_option(
                            "settings-theme-dark",
                            "Dark",
                            true,
                            current == theme::Appearance::Dark,
                            theme::Appearance::Dark,
                            cx,
                        )),
                )
                .child(render_theme_option(
                    "settings-theme-system",
                    "System",
                    theme::is_dark(),
                    current == theme::Appearance::System,
                    theme::Appearance::System,
                    cx,
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
    pref: theme::Appearance,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let border = if selected {
        theme::accent()
    } else {
        theme::border()
    };
    // A preview card depicts ONE arm, and must keep depicting it while the app
    // is in the other — so it cannot read live tokens. These literals are the
    // only sanctioned ones outside theme.rs, and they mirror it exactly.
    // (The System card passes `dark_preview = theme::is_dark()`, so it
    // correctly previews whatever the OS currently resolves to.)
    let arm: [Hsla; 7] = if dark_preview {
        [
            gpui::rgb(0x1a1d22).into(), // shell
            gpui::rgb(0x0e1013).into(), // buffer
            gpui::rgb(0x868d99).into(), // muted
            gpui::rgb(0x74ade8).into(), // accent
            gpui::rgb(0xa1c181).into(), // added
            gpui::rgb(0xd07277).into(), // deleted
            gpui::rgb(0xd3d7de).into(), // text — the card labels itself
        ]
    } else {
        [
            gpui::rgb(0xeaeaeb).into(),
            gpui::rgb(0xffffff).into(),
            gpui::rgb(0x6b6d76).into(),
            gpui::rgb(0x4257c9).into(),
            gpui::rgb(0x3f8a3a).into(),
            gpui::rgb(0xc0392e).into(),
            gpui::rgb(0x383a41).into(),
        ]
    };
    let [shell_bg, sidebar_bg, bar_muted, bar_accent, bar_add, bar_del, bar_text] = arm;
    v_flex()
        .id(id)
        .cursor_pointer()
        .on_click(cx.listener(move |app, _evt, window, cx| {
            app.set_appearance(pref, Some(window), cx);
        }))
        .flex_1()
        .max_w(theme::z(274.0))
        .min_h(theme::z(126.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(border)
        .bg(sidebar_bg)
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
                        .child(theme_preview_dot(bar_muted))
                        .child(theme_preview_bar(theme::z(28.0), bar_muted))
                        .child(theme_preview_dot(bar_muted))
                        .child(theme_preview_bar(theme::z(34.0), bar_muted)),
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
                                .child(theme_preview_bar(theme::z(38.0), bar_muted))
                                .child(theme_preview_bar(theme::z(32.0), bar_muted))
                                .child(theme_preview_bar(theme::z(42.0), bar_accent)),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .h_full()
                                .p(theme::z(8.0))
                                .gap(theme::z(5.0))
                                .child(theme_preview_bar(theme::z(72.0), bar_add))
                                .child(theme_preview_bar(theme::z(48.0), bar_del))
                                .child(theme_preview_bar(theme::z(60.0), bar_del)),
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
                        .border_color(if selected { bar_accent } else { bar_muted })
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(selected, |el| {
                            el.child(
                                div()
                                    .w(theme::z(7.0))
                                    .h(theme::z(7.0))
                                    .rounded_full()
                                    .bg(bar_accent),
                            )
                        }),
                )
                .child(
                    div()
                        .text_size(theme::z(13.0))
                        .text_color(bar_text)
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
        .child(
            h_flex()
                .w_full()
                .gap(theme::z(14.0))
                .items_start()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(render_model_group(app, window, cx)),
                )
                .child(div().flex_1().min_w_0().child(render_text_input(
                    app,
                    window,
                    cx,
                    "settings-ai-api-key",
                    SettingsField::AiApiKey,
                    "API Key",
                    app.settings.ai.provider.api_key_hint(),
                    true,
                    false,
                    false,
                    None,
                ))),
        )
        .child(render_endpoint_group(app, window, cx))
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
            false,
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
        .min_h(theme::z(52.0))
        .p(theme::z(8.0))
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
            theme::bg()
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
        // The catalogue is a live network fetch, so failure is routine — no
        // network, bad key, rate limit. Show the model that is STILL
        // configured alongside the error, or the panel reads as though AI is
        // broken when in fact the saved model keeps working.
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
                v_flex()
                    .w_full()
                    .gap(theme::z(3.0))
                    .child(
                        div()
                            .text_size(theme::z(11.0))
                            .text_color(theme::text_muted())
                            .child("Still using your saved model"),
                    )
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::text_main())
                            .child(if app.settings.ai.model.trim().is_empty() {
                                "None set \u{2014} type a model id in the field above."
                                    .to_string()
                            } else {
                                app.settings.ai.model.clone()
                            }),
                    ),
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
                        .h(theme::z(176.0))
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
        .bg(theme::bg())
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
    let model_row_id = stable_id_slug(&model.id);

    h_flex()
        .id(SharedString::from(format!("settings-model-{model_row_id}")))
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
                    theme::on_accent()
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
                            .bg(theme::on_accent()),
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
                            theme::on_accent()
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
                            theme::with_alpha(theme::on_accent(), 0.7)
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
                    .bg(theme::bg())
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

fn render_section_subhead(title: &str, description: &str) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(4.0))
        .child(
            div()
                .text_size(theme::z(13.0))
                .text_color(theme::text_main())
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(theme::z(11.0))
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
    disabled: bool,
    note: Option<&str>,
) -> impl IntoElement {
    use crate::ui::text_field;

    let disabled = disabled || app.settings_field_read_only(field);
    let value = app.settings_field_value(field);
    let cursor = app.settings_field_cursor(field).min(value.len());
    let selection = app.settings_field_selection(field);
    let focused = app.settings_field_focused(field, window) && !disabled;

    let display_value = if password && !focused {
        mask_password(value)
    } else {
        value.to_string()
    };
    let large_multiline = matches!(field, SettingsField::IgnoredFiles);

    let text = if disabled {
        div()
            .text_size(theme::z(12.0))
            .text_color(theme::text_muted())
            .truncate()
            .child(if display_value.is_empty() {
                placeholder.to_string()
            } else {
                display_value.clone()
            })
    } else {
        text_field::render_text_content(
            &display_value,
            cursor,
            selection,
            focused,
            placeholder,
            multiline,
        )
    };

    let field_shell = div()
        .id(id)
        .track_focus(&app.settings_modal.focus)
        .key_context("text-field")
        .on_key_down(cx.listener(GitSparkApp::handle_settings_key))
        .w_full()
        .when(large_multiline, |el| el.h(theme::z(260.0)))
        .when(multiline && !large_multiline, |el| el.h(theme::z(54.0)))
        .when(!multiline, |el| el.min_h(theme::z(36.0)))
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
        .text_color(if disabled {
            theme::text_muted()
        } else {
            theme::text_main()
        })
        .when(multiline, |el| el.overflow_hidden())
        .when(!disabled, |el| el.cursor_text())
        .child(text)
        .when(!disabled, |el| {
            el.on_click(cx.listener(move |app, _evt, window, cx| {
                app.activate_settings_field(field, window, cx);
            }))
        });

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

#[cfg(test)]
mod tests {
    use super::settings_modal_geometry;

    #[test]
    fn settings_modal_fits_inside_minimum_window() {
        let (width, height, left, top) = settings_modal_geometry(720.0, 480.0);

        assert_eq!(width, 688.0);
        assert_eq!(height, 448.0);
        assert_eq!(left, 16.0);
        assert_eq!(top, 16.0);
        assert!(left + width <= 720.0);
        assert!(top + height <= 480.0);
    }

    #[test]
    fn settings_modal_uses_preferred_size_when_space_allows() {
        let (width, height, left, top) = settings_modal_geometry(1000.0, 700.0);

        assert_eq!(width, 940.0);
        assert_eq!(height, 668.0);
        assert_eq!(left, 30.0);
        assert_eq!(top, 16.0);
    }
}
