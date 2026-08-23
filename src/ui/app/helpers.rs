use super::*;

pub(super) fn render_branch_switch_option(
    id: &'static str,
    selected: bool,
    title: impl Into<String>,
    description: &'static str,
    bring_changes: bool,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .w_full()
        .min_h(theme::z(72.0))
        .p(theme::z(12.0))
        .gap(theme::z(10.0))
        .items_start()
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
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_bg()))
        .child(
            div()
                .w(theme::z(16.0))
                .h(theme::z(16.0))
                .mt(theme::z(2.0))
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
                            .w(theme::z(8.0))
                            .h(theme::z(8.0))
                            .rounded_full()
                            .bg(theme::accent()),
                    )
                }),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(theme::z(4.0))
                .child(
                    div()
                        .text_size(theme::z(13.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.into()),
                )
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_muted())
                        .child(description),
                ),
        )
        .on_click(cx.listener(move |app, _evt, _win, cx| {
            app.repo.switch_branch_bring_changes = bring_changes;
            cx.notify();
        }))
}

pub(super) fn short_commit_label(oid: &str) -> &str {
    &oid[..oid.len().min(7)]
}

pub(super) fn branch_comparison_message(comparison: &BranchComparison) -> String {
    if comparison.ahead == 0 && comparison.behind == 0 {
        return format!(
            "'{}' is up to date with '{}'.",
            comparison.current_branch, comparison.target_branch
        );
    }

    let ahead_unit = if comparison.ahead == 1 {
        "commit"
    } else {
        "commits"
    };
    let behind_unit = if comparison.behind == 1 {
        "commit"
    } else {
        "commits"
    };
    format!(
        "'{}' is {} {ahead_unit} ahead and {} {behind_unit} behind '{}'.",
        comparison.current_branch, comparison.ahead, comparison.behind, comparison.target_branch
    )
}

pub(super) fn tag_name_length_validation_message(tag_name: &str) -> Option<String> {
    (tag_name.len() > MAX_TAG_NAME_LENGTH)
        .then(|| format!("The tag name cannot be longer than {MAX_TAG_NAME_LENGTH} characters"))
}

pub(super) fn sanitized_ref_name(name: &str) -> String {
    let mut sanitized = name
        .chars()
        .map(|ch| {
            if ch <= '\u{20}'
                || ch == '\u{7f}'
                || matches!(
                    ch,
                    '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '|' | '"' | '<' | '>'
                )
            {
                '-'
            } else {
                ch
            }
        })
        .collect::<String>();

    while sanitized.contains("@{") {
        sanitized = sanitized.replace("@{", "-");
    }
    while sanitized.contains("..") {
        sanitized = sanitized.replace("..", "-");
    }
    if sanitized.starts_with('.') {
        sanitized.replace_range(..1, "-");
    }
    if sanitized.ends_with(".lock") {
        let start = sanitized.len() - ".lock".len();
        sanitized.replace_range(start.., "-");
    }
    if sanitized.ends_with('.') || sanitized.ends_with('/') {
        let start = sanitized
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
        sanitized.replace_range(start.., "-");
    }

    sanitized
        .trim_start_matches(|ch| matches!(ch, '-' | '+'))
        .to_string()
}

pub(super) fn branch_validation_message_color(message: &str) -> Hsla {
    if message.starts_with("Will be ") {
        theme::warning()
    } else {
        theme::danger()
    }
}

pub(super) fn default_commit_summary_for_change(change: &ChangeEntry) -> String {
    let filename = change.path.rsplit('/').next().unwrap_or(&change.path);
    let verb = if change.status.contains('?') || change.status.contains('A') {
        "Create"
    } else if change.status.contains('D') {
        "Delete"
    } else {
        "Update"
    };
    format!("{verb} {filename}")
}

pub(super) fn pluralize_files(count: usize) -> String {
    match count {
        0 => "no listed files".to_string(),
        1 => "1 file".to_string(),
        count => format!("{count} files"),
    }
}

pub(crate) fn diff_line_stats(diffs: &[DiffEntry]) -> (usize, usize) {
    let mut added = 0;
    let mut deleted = 0;

    for diff in diffs {
        for line in diff.diff.lines() {
            if line.starts_with("+++") || line.starts_with("---") {
                continue;
            }
            if line.starts_with('+') {
                added += 1;
            } else if line.starts_with('-') {
                deleted += 1;
            }
        }
    }

    (added, deleted)
}

pub(super) fn commit_diff_clipboard_text(diffs: &[DiffEntry]) -> String {
    diffs
        .iter()
        .map(|entry| {
            let body = entry.diff.trim_end();
            if body.starts_with("diff --git ")
                || body.starts_with("--- ")
                || body.starts_with("Binary file")
                || body.starts_with("Binary files")
            {
                body.to_string()
            } else {
                format!("FILE: {}\n{body}", entry.path)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(super) fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub(super) fn external_command_from_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn spawn_shell_path_command(command: &str, path: &Path) -> std::io::Result<()> {
    spawn_shell_arg_command(command, &path.to_string_lossy())
}

pub(super) fn spawn_shell_arg_command(command: &str, arg: &str) -> std::io::Result<()> {
    spawn_shell_arg_command_with_shell("sh", command, arg)
}

/// Same as [`spawn_shell_path_command`], but with the shell binary
/// configurable — the Settings modal's "Shell" override applies here, since
/// this is what actually runs the (possibly-overridden) editor command.
pub(super) fn spawn_shell_path_command_with_shell(
    shell: &str,
    command: &str,
    path: &Path,
) -> std::io::Result<()> {
    spawn_shell_arg_command_with_shell(shell, command, &path.to_string_lossy())
}

fn spawn_shell_arg_command_with_shell(
    shell: &str,
    command: &str,
    arg: &str,
) -> std::io::Result<()> {
    Command::new(shell)
        .arg("-lc")
        .arg(format!("{} {}", command, shell_escape(arg)))
        .spawn()
        .map(|_| ())
}

pub(super) fn branch_switch_needs_stash(error: &str) -> bool {
    let normalized = error.to_lowercase();
    normalized.contains("would be overwritten by checkout")
        || normalized.contains("would be overwritten by merge")
        || normalized.contains("please commit your changes or stash them")
}

pub(super) fn identity_settings_focus_field_for(identity: &GitIdentity) -> SettingsField {
    let missing_name = identity.user_name.trim().is_empty();

    if missing_name || !git_author_name_is_valid(&identity.user_name) {
        SettingsField::GitUserName
    } else {
        SettingsField::GitUserEmail
    }
}

pub(super) fn reveal_path(path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(command) = external_command_from_env("GITSPARK_REVEAL_COMMAND") {
        return spawn_shell_path_command(&command, path).map_err(Into::into);
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(open::that_detached(path)?)
    }
}

pub(super) fn open_with_default_program(
    path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(command) = external_command_from_env("GITSPARK_OPEN_COMMAND") {
        return spawn_shell_path_command(&command, path).map_err(Into::into);
    }

    Ok(open::that(path)?)
}

pub(super) fn open_url(url: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(command) = external_command_from_env("GITSPARK_OPEN_URL_COMMAND") {
        return spawn_shell_arg_command(&command, url).map_err(Into::into);
    }

    Ok(open::that_detached(url)?)
}
#[cfg(test)]
mod tests {
    use super::{
        GitIdentity, MAX_TAG_NAME_LENGTH, identity_settings_focus_field_for, sanitized_ref_name,
        tag_name_length_validation_message,
    };
    use crate::ui::settings_modal::SettingsField;

    #[test]
    fn validates_github_desktop_tag_name_length_limit() {
        assert!(tag_name_length_validation_message(&"x".repeat(MAX_TAG_NAME_LENGTH)).is_none());
        assert_eq!(
            tag_name_length_validation_message(&"x".repeat(MAX_TAG_NAME_LENGTH + 1)),
            Some("The tag name cannot be longer than 245 characters".to_string())
        );
    }

    #[test]
    fn sanitizes_branch_names_like_github_desktop() {
        assert_eq!(sanitized_ref_name("feature branch?"), "feature-branch-");
        assert_eq!(sanitized_ref_name("+ bad/name.lock"), "bad/name-");
        assert_eq!(sanitized_ref_name(".@{bad..name."), "bad-name-");
        assert_eq!(sanitized_ref_name("////"), "///-");
        assert_eq!(sanitized_ref_name("   "), "");
    }

    #[test]
    fn focuses_missing_or_invalid_identity_field() {
        let mut identity = GitIdentity {
            user_name: String::new(),
            user_email: "dev@example.test".to_string(),
            pull_rebase: None,
            default_branch: None,
        };
        assert!(matches!(
            identity_settings_focus_field_for(&identity),
            SettingsField::GitUserName
        ));

        identity.user_name = ".".to_string();
        assert!(matches!(
            identity_settings_focus_field_for(&identity),
            SettingsField::GitUserName
        ));

        identity.user_name = "GitSpark Dev".to_string();
        identity.user_email.clear();
        assert!(matches!(
            identity_settings_focus_field_for(&identity),
            SettingsField::GitUserEmail
        ));
    }
}
