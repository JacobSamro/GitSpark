//! Syntax highlighting for diff content (design.md §5, the syntax hues).
//!
//! ## Why scopes rather than a syntect theme
//!
//! syntect ships themes, but ours is already pinned by the design spec — six
//! exact hues per arm, and a light arm that is re-picked rather than a flipped
//! dark one. So this maps syntect's *scopes* onto our own tokens and ignores
//! its themes entirely. That also means highlighting survives a theme switch
//! for free: what gets cached is the token CLASS, not a colour.
//!
//! ## Why it caches
//!
//! `parse_diff` runs on the render path, and this used to be a project where
//! git ran there too — that was measured at 1.75s of blocked UI and fixed. A
//! full parser pass per frame would put the cost straight back. Highlighting
//! is therefore memoised per (syntax, line), so a line is parsed once no
//! matter how many frames it survives, and rendering is a slice plus a colour
//! lookup.
//!
//! ## What it does not do
//!
//! Each line is parsed on its own, with no state carried across lines. A diff
//! shows fragments — a hunk can start in the middle of a block comment or a
//! multi-line string with the opening delimiter nowhere in the file we were
//! given — so there is no honest state to carry. The cost is that an unclosed
//! construct is highlighted as if it closed at the line end. Every diff viewer
//! that highlights hunks has this property; the alternative is re-parsing the
//! whole file for context we often do not have.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;

use gpui::Hsla;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

use super::theme;

/// Which of the design spec's syntax colours a run takes.
///
/// Deliberately coarse. The spec defines six hues, so resolving syntect's
/// hundreds of scopes into anything finer would invent distinctions the
/// palette cannot express.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenClass {
    /// Unclassified — keeps the surrounding text colour.
    Plain,
    Keyword,
    Function,
    String,
    Type,
    Comment,
    Number,
}

impl TokenClass {
    /// The colour for this class, or `None` to keep the caller's base colour.
    ///
    /// Returns `None` for `Plain` rather than `text_main()` so a diff row can
    /// keep its own foreground — deleted and added rows do not necessarily
    /// share one base.
    pub fn color(self) -> Option<Hsla> {
        match self {
            TokenClass::Plain => None,
            TokenClass::Keyword => Some(theme::syntax_keyword()),
            TokenClass::Function => Some(theme::syntax_function()),
            TokenClass::String => Some(theme::syntax_string()),
            TokenClass::Type => Some(theme::syntax_type()),
            TokenClass::Comment => Some(theme::syntax_comment()),
            TokenClass::Number => Some(theme::syntax_number()),
        }
    }
}

/// One highlighted run: a byte length and the class covering it.
///
/// Lengths rather than absolute ranges so a run can be sliced straight out of
/// the line while walking, and so the spans stay valid if the same content
/// appears at a different offset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    pub len: usize,
    pub class: TokenClass,
}

// ---------------------------------------------------------------------------
// Syntax set
// ---------------------------------------------------------------------------

/// The syntax set, from `two-face` rather than syntect's own defaults.
///
/// syntect bundles 75 syntaxes and among the ones it does NOT carry are Swift,
/// TypeScript, TSX, Kotlin and TOML — enough that a lot of real repositories
/// would show an entirely grey diff. `two-face` packages the set `bat` ships,
/// which covers all of those.
///
/// The newlines variant is the one that can parse a line at a time.
fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

/// The syntax for a path, by extension, or `None` when nothing matches.
///
/// `None` is a normal outcome, not a failure: plain text, lockfiles and
/// unknown extensions all land here and simply render unhighlighted.
pub fn syntax_for_path(path: &str) -> Option<&'static SyntaxReference> {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())?;
    syntax_set().find_syntax_by_extension(extension)
}

// ---------------------------------------------------------------------------
// Scope mapping
// ---------------------------------------------------------------------------

/// The scope prefixes we recognise, most specific first.
///
/// Order matters: `entity.name.function` has to be tested before
/// `entity.name`, or every function name would come out as a type.
fn scope_table() -> &'static [(Scope, TokenClass)] {
    static TABLE: OnceLock<Vec<(Scope, TokenClass)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let entries: &[(&str, TokenClass)] = &[
            ("comment", TokenClass::Comment),
            ("string", TokenClass::String),
            ("constant.numeric", TokenClass::Number),
            ("constant.language", TokenClass::Number),
            ("constant.character", TokenClass::String),
            ("entity.name.function", TokenClass::Function),
            ("support.function", TokenClass::Function),
            ("meta.function-call", TokenClass::Function),
            ("variable.function", TokenClass::Function),
            ("entity.name.type", TokenClass::Type),
            ("entity.name.class", TokenClass::Type),
            ("entity.name.struct", TokenClass::Type),
            ("entity.name.enum", TokenClass::Type),
            ("entity.name.trait", TokenClass::Type),
            ("entity.other.inherited-class", TokenClass::Type),
            ("support.type", TokenClass::Type),
            ("support.class", TokenClass::Type),
            ("storage.type", TokenClass::Type),
            ("storage.modifier", TokenClass::Keyword),
            ("keyword", TokenClass::Keyword),
            ("variable.language", TokenClass::Keyword),
        ];
        entries
            .iter()
            .filter_map(|(text, class)| Scope::new(text).ok().map(|scope| (scope, *class)))
            .collect()
    })
}

/// Resolve a scope stack to a class by walking from the innermost scope out.
///
/// Innermost first because the narrowest scope is the most specific: a number
/// inside a function call should read as a number, not as a function.
fn class_for(stack: &ScopeStack) -> TokenClass {
    for scope in stack.as_slice().iter().rev() {
        for (prefix, class) in scope_table() {
            if prefix.is_prefix_of(*scope) {
                return *class;
            }
        }
    }
    TokenClass::Plain
}

// ---------------------------------------------------------------------------
// Highlighting, memoised
// ---------------------------------------------------------------------------

thread_local! {
    static CACHE: RefCell<HashMap<(String, String), Arc<Vec<Span>>>> =
        RefCell::new(HashMap::new());
}

/// Above this many entries the cache is dropped wholesale.
///
/// A plain clear rather than an LRU: this exists to stop a long session from
/// growing without bound, and the cost of refilling it is one parse per
/// visible line. Tracking recency would cost more than it saves.
const CACHE_LIMIT: usize = 20_000;

/// Highlight one line, returning runs that exactly cover it.
///
/// The returned spans' lengths always sum to `line.len()`, so a caller can
/// walk them and slice without bounds checks.
pub fn highlight_line(line: &str, syntax: &SyntaxReference) -> Arc<Vec<Span>> {
    let key = (syntax.name.clone(), line.to_string());

    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }

    let spans = Arc::new(parse_line(line, syntax));

    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, Arc::clone(&spans));
    });

    spans
}

fn parse_line(line: &str, syntax: &SyntaxReference) -> Vec<Span> {
    let plain = vec![Span {
        len: line.len(),
        class: TokenClass::Plain,
    }];
    if line.is_empty() {
        return Vec::new();
    }

    let set = syntax_set();
    let mut state = ParseState::new(syntax);
    // syntect's line parser expects the trailing newline the "newlines"
    // syntax set was built for; without it some rules never close.
    let owned = format!("{line}\n");
    let Ok(ops) = state.parse_line(&owned, set) else {
        return plain;
    };

    let mut stack = ScopeStack::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut cursor = 0usize;

    for (offset, op) in ops {
        // Clamp: the newline we appended is not part of the caller's line, and
        // an op can land on it.
        let offset = offset.min(line.len());
        if offset > cursor {
            push_span(&mut spans, offset - cursor, class_for(&stack));
            cursor = offset;
        }
        if stack.apply(&op).is_err() {
            // A malformed op invalidates the stack, so anything after it would
            // be classified against nonsense. Give up on the line rather than
            // colour it wrongly.
            return plain;
        }
    }

    if cursor < line.len() {
        push_span(&mut spans, line.len() - cursor, class_for(&stack));
    }

    spans
}

/// Append a run, merging into the previous one when the class is unchanged.
///
/// Without merging, syntect's per-token ops produce a separate run for every
/// identifier and space, and each run becomes its own element in the row.
fn push_span(spans: &mut Vec<Span>, len: usize, class: TokenClass) {
    if len == 0 {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.class == class
    {
        last.len += len;
        return;
    }
    spans.push(Span { len, class });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust() -> &'static SyntaxReference {
        syntax_for_path("x.rs").expect("rust syntax ships with syntect")
    }

    /// The invariant every caller slices against.
    fn assert_covers(line: &str, spans: &[Span]) {
        let total: usize = spans.iter().map(|s| s.len).sum();
        assert_eq!(
            total,
            line.len(),
            "spans must cover the line exactly: {spans:?} for {line:?}"
        );
    }

    #[test]
    fn recognises_common_rust_constructs() {
        let line = "let x = 42; // note";
        let spans = highlight_line(line, rust());
        assert_covers(line, &spans);

        let classes: Vec<TokenClass> = spans.iter().map(|s| s.class).collect();
        assert!(
            classes.contains(&TokenClass::Keyword),
            "`let` should be a keyword: {classes:?}"
        );
        assert!(
            classes.contains(&TokenClass::Number),
            "`42` should be a number: {classes:?}"
        );
        assert!(
            classes.contains(&TokenClass::Comment),
            "the trailing comment should be a comment: {classes:?}"
        );
    }

    #[test]
    fn a_string_literal_is_one_run() {
        let line = r#"let s = "hello world";"#;
        let spans = highlight_line(line, rust());
        assert_covers(line, &spans);
        assert!(
            spans.iter().any(|s| s.class == TokenClass::String),
            "{spans:?}"
        );
    }

    #[test]
    fn spans_cover_lines_with_multibyte_characters() {
        // Byte lengths, not char counts — slicing on a char boundary is the
        // difference between a highlighted line and a panic.
        let line = "let emoji = \"héllo → 🌍\"; // café";
        let spans = highlight_line(line, rust());
        assert_covers(line, &spans);

        // Every boundary must be a real char boundary, or slicing panics.
        let mut at = 0usize;
        for span in spans.iter() {
            at += span.len;
            assert!(
                line.is_char_boundary(at),
                "span boundary {at} splits a character in {line:?}"
            );
        }
    }

    #[test]
    fn a_whole_line_comment_is_one_span_that_is_still_a_comment() {
        // The renderer has a fast path for lines that need only one colour.
        // It used to key off `spans.len() <= 1` alone and paint them in the
        // row's base colour, which silently dropped the comment tint from
        // every full-line comment. The class on a lone span matters.
        let line = "// just a comment across the whole line";
        let spans = highlight_line(line, rust());
        assert_covers(line, &spans);
        assert_eq!(spans.len(), 1, "expected a single run: {spans:?}");
        assert_eq!(
            spans[0].class,
            TokenClass::Comment,
            "a lone span still carries its class"
        );
        assert!(
            spans[0].class.color().is_some(),
            "a comment must resolve to a colour, not fall through to the base"
        );
    }

    #[test]
    fn an_empty_line_has_no_spans() {
        assert!(highlight_line("", rust()).is_empty());
    }

    #[test]
    fn indentation_is_preserved_in_the_spans() {
        // Leading whitespace is part of the line and must survive, or every
        // highlighted diff row loses its indentation.
        let line = "        return value;";
        let spans = highlight_line(line, rust());
        assert_covers(line, &spans);
        assert!(
            spans[0].len >= 8,
            "leading indentation should not be dropped: {spans:?}"
        );
    }

    #[test]
    fn unknown_extensions_have_no_syntax() {
        assert!(syntax_for_path("Cargo.lock").is_none());
        assert!(syntax_for_path("no-extension").is_none());
    }

    #[test]
    fn repeated_lines_come_back_from_the_cache() {
        let line = "fn main() {}";
        let first = highlight_line(line, rust());
        let second = highlight_line(line, rust());
        assert!(
            Arc::ptr_eq(&first, &second),
            "the second call should be a cache hit, not a reparse"
        );
    }

    #[test]
    fn adjacent_runs_of_one_class_are_merged() {
        // Plain text should not fragment into one run per token; the row would
        // otherwise build dozens of elements for a line of ordinary code.
        let line = "        aaa bbb ccc ddd eee";
        let spans = highlight_line(line, rust());
        assert_covers(line, &spans);
        assert!(
            spans.len() <= 4,
            "expected merged runs, got {}: {spans:?}",
            spans.len()
        );
    }
}

#[cfg(test)]
mod coverage {
    use super::*;
    #[test]
    fn covers_the_languages_this_app_is_used_on() {
        // syntect's own defaults miss all of these; two-face is here to
        // provide them, and this test is what keeps that true.
        for ext in [
            "swift", "ts", "tsx", "kt", "toml", "rs", "js", "py", "go", "java", "rb", "c", "cpp",
            "cs", "sh", "yaml", "json", "md",
        ] {
            assert!(
                syntax_for_path(&format!("x.{ext}")).is_some(),
                ".{ext} has no syntax"
            );
        }
    }
}
