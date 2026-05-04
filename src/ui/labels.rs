pub(crate) fn changed_files(count: usize) -> String {
    if count == 1 {
        "1 changed file".to_string()
    } else {
        format!("{count} changed files")
    }
}

pub(crate) fn commit_files(count: usize) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

pub(crate) fn included_changed_files(included: usize, total: usize) -> String {
    format!("{included} of {}", changed_files(total))
}

pub(crate) fn discard_changes_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Discard Changes…"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Discard changes…"
    }
}

pub(crate) fn ignore_file_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Ignore File (Add to .gitignore)"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Ignore file (add to .gitignore)"
    }
}

pub(crate) fn ignore_all_extension_menu(extension: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        format!("Ignore All .{extension} Files (Add to .gitignore)")
    }

    #[cfg(not(target_os = "macos"))]
    {
        format!("Ignore all .{extension} files (add to .gitignore)")
    }
}

pub(crate) fn copy_file_path_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Copy File Path"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Copy file path"
    }
}

pub(crate) fn copy_relative_file_path_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Copy Relative File Path"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Copy relative file path"
    }
}

pub(crate) fn reveal_in_file_manager_menu() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "Show in Explorer"
    }

    #[cfg(target_os = "macos")]
    {
        "Reveal in Finder"
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "Show in your File Manager"
    }
}

pub(crate) fn open_in_external_editor_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Open in External Editor"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Open in external editor"
    }
}

pub(crate) fn open_with_default_program_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Open with Default Program"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Open with default program"
    }
}

pub(crate) fn reset_to_commit_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Reset to Commit…"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Reset to commit…"
    }
}

pub(crate) fn checkout_commit_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Checkout Commit"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Checkout commit"
    }
}

pub(crate) fn reorder_commit_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Reorder Commit"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Reorder commit"
    }
}

pub(crate) fn revert_changes_in_commit_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Revert Changes in Commit"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Revert changes in commit"
    }
}

pub(crate) fn create_branch_from_commit_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Create Branch from Commit"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Create branch from commit"
    }
}

pub(crate) fn create_tag_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Create Tag…"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Create tag…"
    }
}

pub(crate) fn cherry_pick_commit_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Cherry-pick Commit…"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Cherry-pick commit…"
    }
}

#[allow(dead_code)]
pub(crate) fn rename_branch_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Rename Branch…"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Rename branch…"
    }
}

pub(crate) fn rename_branch_context_menu() -> &'static str {
    "Rename…"
}

pub(crate) fn copy_branch_name_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Copy Branch Name"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Copy branch name"
    }
}

pub(crate) fn view_branch_on_github_menu() -> &'static str {
    "View Branch on GitHub"
}

#[allow(dead_code)]
pub(crate) fn delete_branch_menu() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Delete Branch…"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Delete branch…"
    }
}

pub(crate) fn delete_branch_context_menu() -> &'static str {
    "Delete…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pluralizes_file_counts() {
        assert_eq!(changed_files(0), "0 changed files");
        assert_eq!(changed_files(1), "1 changed file");
        assert_eq!(changed_files(2), "2 changed files");
        assert_eq!(commit_files(1), "1 file");
        assert_eq!(commit_files(2), "2 files");
        assert_eq!(included_changed_files(0, 1), "0 of 1 changed file");
        assert_eq!(included_changed_files(1, 2), "1 of 2 changed files");
    }

    #[test]
    fn uses_platform_menu_label_casing() {
        #[cfg(target_os = "macos")]
        {
            assert_eq!(discard_changes_menu(), "Discard Changes…");
            assert_eq!(ignore_file_menu(), "Ignore File (Add to .gitignore)");
            assert_eq!(
                ignore_all_extension_menu("rs"),
                "Ignore All .rs Files (Add to .gitignore)"
            );
            assert_eq!(copy_file_path_menu(), "Copy File Path");
            assert_eq!(copy_relative_file_path_menu(), "Copy Relative File Path");
            assert_eq!(reveal_in_file_manager_menu(), "Reveal in Finder");
            assert_eq!(open_in_external_editor_menu(), "Open in External Editor");
            assert_eq!(
                open_with_default_program_menu(),
                "Open with Default Program"
            );
            assert_eq!(reset_to_commit_menu(), "Reset to Commit…");
            assert_eq!(checkout_commit_menu(), "Checkout Commit");
            assert_eq!(reorder_commit_menu(), "Reorder Commit");
            assert_eq!(revert_changes_in_commit_menu(), "Revert Changes in Commit");
            assert_eq!(
                create_branch_from_commit_menu(),
                "Create Branch from Commit"
            );
            assert_eq!(create_tag_menu(), "Create Tag…");
            assert_eq!(cherry_pick_commit_menu(), "Cherry-pick Commit…");
            assert_eq!(rename_branch_menu(), "Rename Branch…");
            assert_eq!(rename_branch_context_menu(), "Rename…");
            assert_eq!(copy_branch_name_menu(), "Copy Branch Name");
            assert_eq!(view_branch_on_github_menu(), "View Branch on GitHub");
            assert_eq!(delete_branch_menu(), "Delete Branch…");
            assert_eq!(delete_branch_context_menu(), "Delete…");
        }

        #[cfg(target_os = "windows")]
        {
            assert_eq!(discard_changes_menu(), "Discard changes…");
            assert_eq!(ignore_file_menu(), "Ignore file (add to .gitignore)");
            assert_eq!(
                ignore_all_extension_menu("rs"),
                "Ignore all .rs files (add to .gitignore)"
            );
            assert_eq!(copy_file_path_menu(), "Copy file path");
            assert_eq!(copy_relative_file_path_menu(), "Copy relative file path");
            assert_eq!(reveal_in_file_manager_menu(), "Show in Explorer");
            assert_eq!(open_in_external_editor_menu(), "Open in external editor");
            assert_eq!(
                open_with_default_program_menu(),
                "Open with default program"
            );
            assert_eq!(reset_to_commit_menu(), "Reset to commit…");
            assert_eq!(checkout_commit_menu(), "Checkout commit");
            assert_eq!(reorder_commit_menu(), "Reorder commit");
            assert_eq!(revert_changes_in_commit_menu(), "Revert changes in commit");
            assert_eq!(
                create_branch_from_commit_menu(),
                "Create branch from commit"
            );
            assert_eq!(create_tag_menu(), "Create tag…");
            assert_eq!(cherry_pick_commit_menu(), "Cherry-pick commit…");
            assert_eq!(rename_branch_menu(), "Rename branch…");
            assert_eq!(rename_branch_context_menu(), "Rename…");
            assert_eq!(copy_branch_name_menu(), "Copy branch name");
            assert_eq!(view_branch_on_github_menu(), "View Branch on GitHub");
            assert_eq!(delete_branch_menu(), "Delete branch…");
            assert_eq!(delete_branch_context_menu(), "Delete…");
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            assert_eq!(discard_changes_menu(), "Discard changes…");
            assert_eq!(ignore_file_menu(), "Ignore file (add to .gitignore)");
            assert_eq!(
                ignore_all_extension_menu("rs"),
                "Ignore all .rs files (add to .gitignore)"
            );
            assert_eq!(copy_file_path_menu(), "Copy file path");
            assert_eq!(copy_relative_file_path_menu(), "Copy relative file path");
            assert_eq!(reveal_in_file_manager_menu(), "Show in your File Manager");
            assert_eq!(open_in_external_editor_menu(), "Open in external editor");
            assert_eq!(
                open_with_default_program_menu(),
                "Open with default program"
            );
            assert_eq!(reset_to_commit_menu(), "Reset to commit…");
            assert_eq!(checkout_commit_menu(), "Checkout commit");
            assert_eq!(reorder_commit_menu(), "Reorder commit");
            assert_eq!(revert_changes_in_commit_menu(), "Revert changes in commit");
            assert_eq!(
                create_branch_from_commit_menu(),
                "Create branch from commit"
            );
            assert_eq!(create_tag_menu(), "Create tag…");
            assert_eq!(cherry_pick_commit_menu(), "Cherry-pick commit…");
            assert_eq!(rename_branch_menu(), "Rename branch…");
            assert_eq!(rename_branch_context_menu(), "Rename…");
            assert_eq!(copy_branch_name_menu(), "Copy branch name");
            assert_eq!(view_branch_on_github_menu(), "View Branch on GitHub");
            assert_eq!(delete_branch_menu(), "Delete branch…");
            assert_eq!(delete_branch_context_menu(), "Delete…");
        }
    }
}
