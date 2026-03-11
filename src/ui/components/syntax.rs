use once_cell::sync::Lazy;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
pub static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

pub fn get_syntax(file_path: &str) -> Option<&'static syntect::parsing::SyntaxReference> {
    SYNTAX_SET.find_syntax_for_file(file_path).unwrap_or(None)
}
