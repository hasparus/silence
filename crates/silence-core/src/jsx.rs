//! What JSX does to the text around an expression container, which decides
//! whether that container's braces can be deleted without anyone noticing.

use silence_langs::Lang;
use tree_sitter::Node;

use crate::rewrite::Span;

/// A `{ … }` holding nothing but comments, together with the boundaries of the
/// text runs on either side of it. The boundaries are a fact about the parse;
/// whether the braces may go is a question only [`crate::strip`] can answer,
/// because it depends on which comments are actually being removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Container {
    pub(crate) span: Span,
    pub(crate) run_start: usize,
    pub(crate) run_end: usize,
}

impl Container {
    pub(crate) fn shift(&mut self, byte_off: usize, row_off: usize) {
        self.span.start_byte += byte_off;
        self.span.end_byte += byte_off;
        self.span.start_row += row_off;
        self.span.end_row += row_off;
        self.run_start += byte_off;
        self.run_end += byte_off;
    }

    /// Whether `next` is the very next thing along, with only a run of text
    /// between the two. Removing both joins that run to its neighbours, so the
    /// pair has to be judged together.
    pub(crate) fn adjoins(self, next: Container) -> bool {
        self.run_end == next.span.start_byte && next.run_start == self.span.end_byte
    }
}

/// The container this comment is the entire content of, if there is one.
///
/// Child position only: in `<Foo bar={/* c */} />` the braces are the
/// attribute's value and `bar=` alone does not parse, and in `<Foo {/* c */} />`
/// they are a spread. Both keep their braces. The delimiters must also really
/// be there — on a partial parse the node can run to the end of the file.
pub(crate) fn container(node: Node, source: &str, lang: Lang) -> Option<Container> {
    if !matches!(lang, Lang::Tsx | Lang::JavaScript) {
        return None;
    }
    let parent = node.parent()?;
    if parent.kind() != "jsx_expression" {
        return None;
    }
    if !parent
        .parent()
        .is_some_and(|g| matches!(g.kind(), "jsx_element" | "jsx_fragment"))
    {
        return None;
    }
    let text = source.get(parent.start_byte()..parent.end_byte())?;
    if !text.starts_with('{') || !text.ends_with('}') {
        return None;
    }
    let mut cursor = parent.walk();
    if !parent
        .children(&mut cursor)
        .all(|child| matches!(child.kind(), "{" | "}" | "comment"))
    {
        return None;
    }
    Some(Container {
        span: Span {
            start_byte: parent.start_byte(),
            end_byte: parent.end_byte(),
            start_row: parent.start_position().row + 1,
            end_row: parent.end_position().row + 1,
        },
        run_start: run_start(parent),
        run_end: run_end(parent),
    })
}

/// Whether deleting the braces of every container in `group` — and only those —
/// leaves the rendered markup identical.
///
/// The containers split the text around them into runs. Delete them and the
/// runs join, so the question is exactly whether JSX reads the pieces the same
/// way it reads the whole. Nothing about line positions is guessed; the
/// compilers' own text rule answers it.
///
/// `group` must be the containers actually being removed, in source order. A
/// container that stays keeps its runs apart, so including one here would prove
/// an identity nobody is going to rely on.
pub(crate) fn taking_braces_is_invisible(source: &str, group: &[Container]) -> bool {
    let Some((first, last)) = group.first().zip(group.last()) else {
        return false;
    };
    let mut runs = vec![&source[first.run_start..first.span.start_byte]];
    for pair in group.windows(2) {
        runs.push(&source[pair[0].span.end_byte..pair[1].span.start_byte]);
    }
    runs.push(&source[last.span.end_byte..last.run_end]);

    if runs.iter().any(|run| !is_plain_text(run)) || spells_an_entity(&runs) {
        return false;
    }
    let joined = runs.concat();
    READINGS.iter().all(|&reading| {
        let apart: String = runs.iter().map(|run| clean_text(run, reading)).collect();
        clean_text(&joined, reading) == apart
    })
}

/// JSX's own reading of a run of text: a line's leading and trailing padding
/// goes wherever a line break is on that side, blank lines vanish, and what is
/// left joins with a single space.
///
/// That last part is why a container cannot simply be deleted. Between two
/// lines of text a line break renders as a space; at the edge of a run it
/// renders as nothing. A container splits one run into two, so each break
/// beside it sits at an edge — and joining the runs moves it inside, where it
/// becomes a space that was never there.
///
/// What counts as padding is the one place the compilers part company, so it
/// is a parameter rather than a guess. Babel turns tabs into spaces and trims
/// spaces; TypeScript, esbuild and SWC trim the tab itself.
fn clean_with(text: &str, padding: &[char]) -> String {
    let lines = split_lines(text);
    let last_non_empty = lines
        .iter()
        .rposition(|l| l.contains(|c: char| !padding.contains(&c)))
        .unwrap_or(0);

    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let mut trimmed = *line;
        if i != 0 {
            trimmed = trimmed.trim_start_matches(padding);
        }
        if i != lines.len() - 1 {
            trimmed = trimmed.trim_end_matches(padding);
        }
        if !trimmed.is_empty() {
            out.push_str(trimmed);
            if i != last_non_empty {
                out.push(' ');
            }
        }
    }
    out
}

/// The two readings a file might get compiled under. A cut has to be invisible
/// in both, so agreeing with one of them is not enough.
#[derive(Debug, Clone, Copy)]
enum Reading {
    Babel,
    TypeScript,
}

const READINGS: [Reading; 2] = [Reading::Babel, Reading::TypeScript];

fn clean_text(text: &str, reading: Reading) -> String {
    match reading {
        Reading::Babel => clean_with(&text.replace('\t', " "), &[' ']),
        Reading::TypeScript => clean_with(text, &[' ', '\t']),
    }
}

/// Every line terminator JSX honours, not just the common one: a lone `\r`
/// breaks a line the same way `\n` does.
fn split_lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(['\n', '\r']) {
        out.push(&rest[..i]);
        let width = usize::from(rest[i..].starts_with("\r\n")) + 1;
        rest = &rest[i + width..];
    }
    out.push(rest);
    out
}

/// Text this rule is willing to reason about. Whitespace outside the ASCII
/// kinds is refused: which of those count as line padding is a longer list in
/// TypeScript than in Babel, and modelling the difference buys nothing that
/// real files ask for.
fn is_plain_text(run: &str) -> bool {
    !run.chars()
        .any(|c| c.is_whitespace() && !matches!(c, ' ' | '\t' | '\n' | '\r'))
}

/// Whether joining the runs could spell an entity that no run spells alone.
/// Entities are resolved per run of text, so `&amp` beside `;` is not `&amp;`
/// until the container between them is taken away.
fn spells_an_entity(runs: &[&str]) -> bool {
    (1..runs.len()).any(|split| {
        let left = runs[..split].concat();
        let right = runs[split..].concat();
        let dangling = left
            .rsplit(';')
            .next()
            .is_some_and(|tail| tail.contains('&'));
        dangling && right.contains(';')
    })
}

/// Where the run of text on either side of `container` reaches. A run is not
/// one node: whitespace between two elements sometimes has no node at all, and
/// an entity gets one of its own, so the walk continues until markup ends it
/// and the span is read from the source rather than from any single node.
fn run_start(container: Node) -> usize {
    let mut at = container;
    loop {
        match at.prev_sibling() {
            Some(n) if is_markup(n) => return n.end_byte(),
            Some(n) => at = n,
            None => return at.parent().map_or(at.start_byte(), |p| p.start_byte()),
        }
    }
}

fn run_end(container: Node) -> usize {
    let mut at = container;
    loop {
        match at.next_sibling() {
            Some(n) if is_markup(n) => return n.start_byte(),
            Some(n) => at = n,
            None => return at.parent().map_or(at.end_byte(), |p| p.end_byte()),
        }
    }
}

/// Markup ends a run of text. Anything else beside the braces is read as text,
/// including whatever a partial parse leaves behind.
fn is_markup(node: Node) -> bool {
    matches!(
        node.kind(),
        "jsx_element"
            | "jsx_self_closing_element"
            | "jsx_fragment"
            | "jsx_expression"
            | "jsx_opening_element"
            | "jsx_closing_element"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_cleaned_the_way_jsx_reads_it() {
        assert_eq!(clean_text("Hello", Reading::Babel), "Hello");
        assert_eq!(clean_text("\n    a\n  ", Reading::Babel), "a");
        assert_eq!(clean_text("\n    ", Reading::Babel), "");
        assert_eq!(clean_text(" ", Reading::Babel), " ");
        assert_eq!(clean_text("\n  a\n  b\n", Reading::Babel), "a b");
        assert_eq!(
            clean_text("\n    Signed in as ", Reading::Babel),
            "Signed in as "
        );
    }

    /// A lone `\r` ends a line, so it renders as a space between two words
    /// exactly as `\n` does.
    #[test]
    fn every_line_terminator_ends_a_line() {
        assert_eq!(split_lines("a\r\nb"), vec!["a", "b"]);
        assert_eq!(split_lines("a\rb"), vec!["a", "b"]);
        assert_eq!(split_lines("a\nb"), vec!["a", "b"]);
        assert_eq!(clean_text("a\rb", Reading::Babel), "a b");
        assert_eq!(clean_text("a\r\nb", Reading::Babel), "a b");
    }

    /// Compilers disagree about tabs, so a tab is read both ways rather than
    /// refused: as indentation both erase it, at a line edge they do not.
    #[test]
    fn a_tab_is_read_the_way_each_compiler_reads_it() {
        assert_eq!(clean_text("a\tb", Reading::Babel), "a b");
        assert_eq!(clean_text("a\tb", Reading::TypeScript), "a\tb");
        assert!(is_plain_text("\tb"));
        assert!(!is_plain_text("a\u{a0}b"));
    }

    /// A terminated entity is settled before the join; a dangling one is not.
    #[test]
    fn only_a_dangling_entity_blocks_the_join() {
        assert!(spells_an_entity(&["&amp", ";"]));
        assert!(!spells_an_entity(&["&nbsp;", "x"]));
        assert!(!spells_an_entity(&["a", "b"]));
    }
}
