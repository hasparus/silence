use globset::{Glob, GlobSet, GlobSetBuilder};

pub const DEFAULT_PRESERVE_PATTERNS: &[&str] = &[
    "TODO",
    "FIXME",
    "HACK",
    "XXX",
    "SAFETY",
    "NOTE:",
    "eslint-",
    "prettier-ignore",
    "stylelint-",
    "postcss-",
    "noqa",
    "type: ignore",
    "pylint:",
    "mypy:",
    "nolint",
];

#[derive(Debug, Clone)]
pub struct PreserveConfig {
    literals: Vec<String>,
    globs: Option<GlobSet>,
    directives: bool,
    invalid: Vec<String>,
}

impl Default for PreserveConfig {
    fn default() -> Self {
        Self::with_patterns(
            DEFAULT_PRESERVE_PATTERNS
                .iter()
                .map(|&s| s.to_string())
                .collect(),
        )
        .with_directives(true)
    }
}

impl PreserveConfig {
    #[must_use]
    pub fn empty() -> Self {
        PreserveConfig {
            literals: Vec::new(),
            globs: None,
            directives: false,
            invalid: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_patterns(patterns: Vec<String>) -> Self {
        let mut literals = Vec::new();
        let mut builder = GlobSetBuilder::new();
        let mut invalid = Vec::new();
        let mut any_glob = false;

        for p in patterns {
            if p.contains(['*', '?', '[']) {
                let mut pat = p.clone();
                if !pat.starts_with('*') {
                    pat.insert(0, '*');
                }
                if !pat.ends_with('*') {
                    pat.push('*');
                }
                match Glob::new(&pat) {
                    Ok(g) => {
                        builder.add(g);
                        any_glob = true;
                    }
                    Err(_) => invalid.push(p),
                }
            } else {
                literals.push(p);
            }
        }

        let globs = if any_glob { builder.build().ok() } else { None };
        PreserveConfig {
            literals,
            globs,
            directives: false,
            invalid,
        }
    }

    #[must_use]
    pub fn with_directives(mut self, enabled: bool) -> Self {
        self.directives = enabled;
        self
    }

    #[must_use]
    pub fn invalid_patterns(&self) -> &[String] {
        &self.invalid
    }

    #[must_use]
    pub fn should_preserve(&self, comment_text: &str) -> bool {
        let trimmed = comment_text.trim();
        if self.literals.iter().any(|l| trimmed.contains(l.as_str())) {
            return true;
        }
        if let Some(set) = &self.globs {
            if set.is_match(trimmed) {
                return true;
            }
        }
        if self.directives
            && (looks_like_doc_comment(comment_text)
                || looks_like_directive(comment_text)
                || looks_like_marker(comment_text))
        {
            return true;
        }
        false
    }
}

fn looks_like_doc_comment(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("///") || t.starts_with("//!") || t.starts_with("/**") || t.starts_with("/*!")
}

fn comment_body(text: &str) -> &str {
    let t = text.trim();
    let body = ["/**", "/*!", "/*", "///", "//!", "//", "<!--", "#!", "#"]
        .iter()
        .find_map(|&p| t.strip_prefix(p))
        .unwrap_or(t);
    body.strip_suffix("*/")
        .or_else(|| body.strip_suffix("-->"))
        .unwrap_or(body)
        .trim()
}

fn looks_like_directive(text: &str) -> bool {
    let body = comment_body(text);
    if let Some(rest) = body.strip_prefix('@') {
        return rest.starts_with(char::is_alphabetic);
    }
    if body.starts_with('<') && body.ends_with('>') && body.len() > 2 {
        return true;
    }
    if let Some(token) = body.split_whitespace().next() {
        if let Some((head, tail)) = token.split_once(':') {
            return !head.is_empty()
                && head
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                && tail.starts_with(|c: char| c.is_alphanumeric() || c == '_');
        }
    }
    false
}

/// Markers another tool reads back — `impeccable-variants-start cd383158`,
/// `prettier-ignore-start`, `codegen-end`. Deleting one silently breaks the
/// tool that writes between the pair.
///
/// What makes a marker a marker is the pairing: a sentinel ending in `-start`
/// or `-end` that some tool greps for, optionally followed by an id. Prose does
/// not have that shape, so this asks for it directly rather than guessing from
/// how punctuated a comment looks.
fn looks_like_marker(text: &str) -> bool {
    const PAIRED: [&str; 3] = ["start", "end", "begin"];
    const MAX_IDS: usize = 2;

    let mut tokens = comment_body(text).split_whitespace();
    let Some(sentinel) = tokens.next() else {
        return false;
    };

    let lowered = sentinel.to_ascii_lowercase();
    let paired = PAIRED.iter().any(|suffix| {
        lowered
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.len() > 1 && prefix.ends_with(['-', '_', ':']))
    });

    paired && identifier(sentinel) && tokens.by_ref().take(MAX_IDS).all(identifier) && {
        tokens.next().is_none()
    }
}

fn identifier(token: &str) -> bool {
    !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_machine_markers_are_preserved_by_default() {
        let c = PreserveConfig::default();
        assert!(c.should_preserve("/* impeccable-variants-start cd383158 */"));
        assert!(c.should_preserve("/* impeccable-variants-end cd383158 */"));
        assert!(c.should_preserve("<!-- region-start build-info -->"));
        assert!(c.should_preserve("// codegen-end"));
    }

    #[test]
    fn an_id_that_is_all_digits_still_marks() {
        let c = PreserveConfig::default();
        assert!(c.should_preserve("/* impeccable-variants-start 12345678 */"));
        assert!(c.should_preserve("/* impeccable-variants-end 8675309 */"));
    }

    #[test]
    fn hyphenated_prose_is_not_mistaken_for_a_marker() {
        let c = PreserveConfig::default();
        assert!(!c.should_preserve("// well-known trick here"));
        assert!(!c.should_preserve("// e.g. this one"));
        assert!(!c.should_preserve("// don't do this"));
    }

    /// Short prose carrying punctuation is the most common shape of the comment
    /// this tool exists to delete, so it must not read as machine output.
    #[test]
    fn punctuated_prose_is_not_mistaken_for_a_marker() {
        let c = PreserveConfig::default();
        for comment in [
            "// broken.",
            "// unused.",
            "// legacy.",
            "// no-op.",
            "// wait...",
            "/* fine. */",
            "<!-- old. -->",
            "// half-baked hack.",
            "// two-way binding.",
            "// self-explanatory.",
            "// http://example.com",
            "// src/foo/bar.ts",
            "// end",
            "// the end",
        ] {
            assert!(!c.should_preserve(comment), "{comment} should be stripped");
        }
    }

    #[test]
    fn markers_go_away_with_directive_detection_off() {
        let c = PreserveConfig::with_patterns(vec!["TODO".into()]);
        assert!(!c.should_preserve("/* impeccable-variants-start cd383158 */"));
    }

    #[test]
    fn literal_substring_match() {
        let c = PreserveConfig::with_patterns(vec!["TODO".into()]);
        assert!(c.should_preserve("// TODO: do this"));
        assert!(!c.should_preserve("// ordinary"));
    }

    #[test]
    fn glob_match() {
        let c = PreserveConfig::with_patterns(vec!["*IMPORTANT*".into()]);
        assert!(c.should_preserve("// this is IMPORTANT stuff"));
        assert!(!c.should_preserve("// trivial"));
    }

    #[test]
    fn empty_preserves_nothing() {
        let c = PreserveConfig::empty();
        assert!(!c.should_preserve("// TODO"));
        assert!(!c.should_preserve("// @ts-ignore"));
    }

    #[test]
    fn glob_without_leading_star_still_matches_inside_comment() {
        let c = PreserveConfig::with_patterns(vec!["KEEP*".into()]);
        assert!(c.should_preserve("// KEEP this comment"));
    }

    #[test]
    fn invalid_glob_is_reported_not_silently_dropped() {
        let c = PreserveConfig::with_patterns(vec!["[unclosed".into()]);
        assert_eq!(c.invalid_patterns().len(), 1);
        assert_eq!(c.invalid_patterns()[0], "[unclosed");
    }

    #[test]
    fn directive_shaped_comments_are_preserved_by_default() {
        let c = PreserveConfig::default();
        assert!(c.should_preserve("// @ts-ignore"));
        assert!(c.should_preserve("/* @deprecated */"));
        assert!(c.should_preserve("//go:embed files/*"));
        assert!(c.should_preserve("//nolint:errcheck"));
        assert!(c.should_preserve("/// <reference types=\"node\" />"));
    }

    #[test]
    fn css_lint_directives_are_preserved_by_default() {
        let c = PreserveConfig::default();
        assert!(c.should_preserve("/* stylelint-disable */"));
        assert!(c.should_preserve("/* stylelint-disable-next-line color-no-hex */"));
        assert!(c.should_preserve("/* postcss-custom-properties: off */"));
        assert!(!c.should_preserve("/* ordinary css remark */"));
    }

    #[test]
    fn doc_comments_are_preserved_by_default() {
        let c = PreserveConfig::default();
        assert!(c.should_preserve("/// Public docs."));
        assert!(c.should_preserve("//! Module docs."));
        assert!(c.should_preserve("/** JSDoc docs. */"));
        assert!(c.should_preserve("/*! Crate docs. */"));
    }

    #[test]
    fn ordinary_prose_is_not_treated_as_a_directive() {
        let c = PreserveConfig::default();
        assert!(!c.should_preserve("// just a normal comment"));
        assert!(!c.should_preserve("// http://example.com is a url"));
        assert!(!c.should_preserve("/* a multi word remark */"));
        assert!(!c.should_preserve("<!-- an ordinary html comment -->"));
    }

    #[test]
    fn directive_preservation_is_off_for_with_patterns() {
        let c = PreserveConfig::with_patterns(vec!["TODO".into()]);
        assert!(!c.should_preserve("// @ts-ignore"));
        assert!(!c.should_preserve("/// docs"));
    }
}
