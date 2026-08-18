//! What JSX does to the text around an expression container, which decides
//! whether that container's braces can be deleted without anyone noticing.

use tree_sitter::Node;

/// JSX's own reading of a run of text, as every compiler implements it: tabs
/// become spaces, a line's leading and trailing padding goes wherever a newline
/// is on that side, blank lines vanish, and what is left joins with one space.
///
/// The last part is why a container cannot simply be deleted. Between two lines
/// of text a newline renders as a space; at the edge of a run it renders as
/// nothing. A container splits one run into two, so each newline beside it sits
/// at an edge — and joining the runs moves it inside, where it becomes a space
/// that was never there.
fn clean_text(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let last_non_empty = lines
        .iter()
        .rposition(|l| l.contains(|c: char| c != ' ' && c != '\t'))
        .unwrap_or(0);

    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let mut trimmed = line.replace('\t', " ");
        if i != 0 {
            trimmed = trimmed.trim_start_matches(' ').to_string();
        }
        if i != lines.len() - 1 {
            trimmed = trimmed.trim_end_matches(' ').to_string();
        }
        if !trimmed.is_empty() {
            if i != last_non_empty {
                trimmed.push(' ');
            }
            out.push_str(&trimmed);
        }
    }
    out
}

/// Whether deleting `wrapper`'s braces leaves the rendered markup identical.
///
/// The braces split the text around them into two runs. Delete them and the
/// runs join, so the question is exactly whether JSX reads the pieces the same
/// way it reads the whole: `clean(left) + clean(right) == clean(left + right)`.
/// Nothing about line positions is guessed; the compilers' own rule answers it.
///
/// An `&` anywhere in the text is left alone regardless. Entities are resolved
/// per run, so joining `&amp` to `;` spells an entity that was not there.
pub(crate) fn taking_braces_is_invisible(source: &str, wrapper: Node) -> bool {
    let mut first = wrapper;
    while let Some(prev) = first.prev_sibling() {
        if !is_comment_only(prev, source) {
            break;
        }
        first = prev;
    }
    let mut group = vec![first];
    while let Some(next) = group.last().and_then(Node::next_sibling) {
        if !is_comment_only(next, source) {
            break;
        }
        group.push(next);
    }

    let mut runs = vec![&source[run_start(first)..first.start_byte()]];
    for pair in group.windows(2) {
        runs.push(&source[pair[0].end_byte()..pair[1].start_byte()]);
    }
    let last = group[group.len() - 1];
    runs.push(&source[last.end_byte()..run_end(last)]);

    if runs.iter().any(|run| !is_plain_text(run)) {
        return false;
    }
    let apart: String = runs.iter().copied().map(clean_text).collect();
    clean_text(&runs.concat()) == apart
}

/// Text this rule is willing to reason about: no `&`, because entities resolve
/// per run and joining two runs can spell one that was never written; and no
/// whitespace beyond the ASCII kinds, because which of those count as line
/// padding is where compilers stop agreeing with each other.
fn is_plain_text(run: &str) -> bool {
    !run.contains('&')
        && !run
            .chars()
            .any(|c| c.is_whitespace() && !matches!(c, ' ' | '\t' | '\n' | '\r'))
}

/// A `{ … }` holding nothing but comments — the same shape [`crate::strip`]
/// takes the braces from, asked of a neighbour rather than of this comment.
pub(crate) fn is_comment_only(node: Node, source: &str) -> bool {
    if node.kind() != "jsx_expression" {
        return false;
    }
    let Some(text) = source.get(node.start_byte()..node.end_byte()) else {
        return false;
    };
    if !text.starts_with('{') || !text.ends_with('}') {
        return false;
    }
    let mut cursor = node.walk();
    let mut children = node.children(&mut cursor);
    children.any(|child| child.kind() == "comment")
        && node
            .children(&mut node.walk())
            .all(|child| matches!(child.kind(), "{" | "}" | "comment"))
}

/// Where the run of text on the left of `wrapper` begins. Whitespace between
/// two elements is not always a node of its own, so the run is read from the
/// source between siblings rather than from a node that may not be there.
fn run_start(wrapper: Node) -> usize {
    match wrapper.prev_sibling() {
        Some(n) if is_markup(n) => n.end_byte(),
        Some(n) => n.start_byte(),
        None => wrapper
            .parent()
            .map_or(wrapper.start_byte(), |p| p.start_byte()),
    }
}

fn run_end(wrapper: Node) -> usize {
    match wrapper.next_sibling() {
        Some(n) if is_markup(n) => n.start_byte(),
        Some(n) => n.end_byte(),
        None => wrapper
            .parent()
            .map_or(wrapper.end_byte(), |p| p.end_byte()),
    }
}

/// Markup ends a run of text. Anything else beside the braces counts as text,
/// including whatever a partial parse left behind — reading more text than
/// there is can only make the two sides look unequal, which keeps the braces.
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

    /// The cases every JSX compiler agrees on, and the reason a container
    /// cannot be deleted between two lines of prose.
    #[test]
    fn text_is_cleaned_the_way_jsx_reads_it() {
        assert_eq!(clean_text("Hello"), "Hello");
        assert_eq!(clean_text("\n    a\n  "), "a");
        assert_eq!(clean_text("\n    "), "");
        assert_eq!(clean_text(" "), " ");
        assert_eq!(clean_text("\n  a\n  b\n"), "a b");
        assert_eq!(clean_text("\n    Signed in as "), "Signed in as ");
    }

    /// Two runs of prose read as one word each when a container splits them
    /// and as two words joined by a space when it does not.
    #[test]
    fn a_container_between_two_lines_of_prose_is_load_bearing() {
        assert_eq!(clean_text("\n  a\n  "), "a");
        assert_eq!(clean_text("\n  b\n"), "b");
        assert_eq!(clean_text("\n  a\n  \n  b\n"), "a b");
    }
}
