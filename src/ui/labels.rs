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
}
