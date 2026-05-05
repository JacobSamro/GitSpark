use crate::ui::ids::stable_id_slug;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DiffLineSelectionKind {
    Added,
    Deleted,
}

impl DiffLineSelectionKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Deleted => "deleted",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DiffLineSelection {
    pub path: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub kind: DiffLineSelectionKind,
}

impl DiffLineSelection {
    pub(crate) fn id(&self) -> String {
        let line = self
            .new_line
            .or(self.old_line)
            .map(|line| line.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        format!(
            "diff-line-{}-{}-{}",
            stable_id_slug(&self.path),
            self.kind.as_str(),
            line
        )
    }
}
