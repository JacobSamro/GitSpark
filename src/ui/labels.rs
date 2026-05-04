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
        "Discard Changes"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Discard changes"
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
            assert_eq!(discard_changes_menu(), "Discard Changes");
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
        }

        #[cfg(target_os = "windows")]
        {
            assert_eq!(discard_changes_menu(), "Discard changes");
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
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            assert_eq!(discard_changes_menu(), "Discard changes");
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
        }
    }
}
