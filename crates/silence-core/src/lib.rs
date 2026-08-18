use silence_langs::Lang;
use std::collections::HashSet;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

mod preserve;
mod rewrite;
pub use preserve::{PreserveConfig, DEFAULT_PRESERVE_PATTERNS};
use rewrite::{coalesce_same_line, rewrite, Removal, Span};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to set tree-sitter language: {0}")]
    Language(String),
    #[error("failed to parse source")]
    Parse,
    #[error("invalid comment query: {0}")]
    Query(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineMode {
    #[default]
    Collapse,
    PreserveLines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentKind {
    Line,
    Block,
}

#[derive(Debug, Clone, Copy)]
pub struct CommentKinds {
    pub line: bool,
    pub block: bool,
}

impl Default for CommentKinds {
    fn default() -> Self {
        CommentKinds {
            line: true,
            block: true,
        }
    }
}

impl CommentKinds {
    #[must_use]
    pub fn allows(self, kind: CommentKind) -> bool {
        match kind {
            CommentKind::Line => self.line,
            CommentKind::Block => self.block,
        }
    }
}

/// Which lines of a file a strip applies to. `Ranges` with nothing in it means
/// nothing is in scope, which is why it is not the same value as `All`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Lines {
    #[default]
    All,
    Ranges(Vec<(usize, usize)>),
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub line_mode: LineMode,
    pub preserve: PreserveConfig,
    pub lines: Lines,
    pub kinds: CommentKinds,
}

/// Constructed only by [`find_comments`]: it carries a crate-private note on
/// enclosing syntax, so it cannot be built field by field from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Comment {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_row: usize,
    pub end_row: usize,
    pub text: String,
    pub kind: CommentKind,
    /// Syntax that holds nothing but comments, and so has no reason to outlive
    /// them — currently the `{ }` of a JSX expression. Shared with any sibling
    /// comment under the same braces, because it survives if any of them does.
    pub(crate) enclosed_by: Option<Span>,
}

impl Comment {
    #[must_use]
    pub fn is_multiline(&self) -> bool {
        self.end_row > self.start_row
    }

    fn span(&self) -> Span {
        Span {
            start_byte: self.start_byte,
            end_byte: self.end_byte,
            start_row: self.start_row,
            end_row: self.end_row,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub output: String,
    pub removed: usize,
    pub preserved: usize,
}

/// # Errors
/// Returns an error if the grammar fails to load or the parser cannot ingest the source.
pub fn find_comments(source: &str, lang: Lang) -> Result<Vec<Comment>, Error> {
    let grammar = silence_grammars::ensure(lang).map_err(|e| Error::Language(e.to_string()))?;
    let mut parser = Parser::new();
    parser
        .set_language(&grammar)
        .map_err(|e| Error::Language(e.to_string()))?;
    let tree = parser.parse(source, None).ok_or(Error::Parse)?;

    let query =
        Query::new(&grammar, lang.comment_query()).map_err(|e| Error::Query(e.to_string()))?;
    let line_idx = query.capture_index_for_name("line");
    let block_idx = query.capture_index_for_name("block");
    let comment_idx = query.capture_index_for_name("comment");
    if line_idx.is_none() && block_idx.is_none() && comment_idx.is_none() {
        return Err(Error::Query(
            "missing @line, @block, or @comment capture".into(),
        ));
    }

    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();
    let mut out = Vec::new();

    let mut matches = cursor.matches(&query, tree.root_node(), bytes);
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let node = cap.node;
            let start = node.start_position();
            let end = node.end_position();
            let start_byte = node.start_byte();

            if start_byte == 0 && source[start_byte..node.end_byte()].starts_with("#!") {
                continue;
            }

            let mut end_byte = node.end_byte();
            let mut text = source[start_byte..end_byte].to_string();
            if text.ends_with('\r') {
                text.pop();
                end_byte -= 1;
            }

            let kind = match cap.index {
                i if block_idx == Some(i) => CommentKind::Block,
                i if line_idx == Some(i) => CommentKind::Line,
                i if comment_idx == Some(i) => comment_kind_from_text(&text),
                _ => continue,
            };

            out.push(Comment {
                start_byte,
                end_byte,
                start_row: start.row + 1,
                end_row: end.row + 1,
                text,
                kind,
                enclosed_by: comment_only_wrapper(node, source, lang).map(|w| Span {
                    start_byte: w.start_byte(),
                    end_byte: w.end_byte(),
                    start_row: w.start_position().row + 1,
                    end_row: w.end_position().row + 1,
                }),
            });
        }
    }

    collect_injected(source, tree.root_node(), lang.injections(), &mut out)?;

    out.sort_by_key(|c| c.start_byte);
    Ok(out)
}

/// The delimited node holding this comment and nothing but comments — see
/// [`Lang::comment_only_wrappers`]. It goes when the last comment under it
/// goes, which is a question [`strip`] answers, not this one; a node holding
/// anything else (`{/* note */ value}`) keeps its delimiters.
///
/// Position decides as much as kind: in `<Foo bar={/* c */} />` the braces are
/// the attribute's value rather than packaging around a comment, and `bar=`
/// alone does not parse.
///
/// Those delimiters must really be there: on a malformed or partial parse the
/// node can run to the end of the file, and widening onto that would eat code.
fn comment_only_wrapper<'a>(
    node: tree_sitter::Node<'a>,
    source: &str,
    lang: Lang,
) -> Option<tree_sitter::Node<'a>> {
    let kinds = lang.comment_only_wrappers();
    if kinds.is_empty() {
        return None;
    }
    let parent = node.parent()?;
    if !kinds.contains(&parent.kind()) {
        return None;
    }
    if parent.parent().is_some_and(|g| g.kind() == "jsx_attribute") {
        return None;
    }
    let text = source.get(parent.start_byte()..parent.end_byte())?;
    if !text.starts_with('{') || !text.ends_with('}') {
        return None;
    }
    let mut cursor = parent.walk();
    let only_comments = parent
        .children(&mut cursor)
        .all(|child| matches!(child.kind(), "{" | "}" | "comment"));
    only_comments.then_some(parent)
}

/// Re-parse injected sub-language regions (e.g. Astro frontmatter as TypeScript)
/// and merge their comments back, translating byte/row offsets into the outer file.
fn collect_injected<'a>(
    source: &str,
    root: tree_sitter::Node<'a>,
    injections: &[(&str, Lang)],
    out: &mut Vec<Comment>,
) -> Result<(), Error> {
    if injections.is_empty() {
        return Ok(());
    }
    let mut stack: Vec<tree_sitter::Node<'a>> = vec![root];
    while let Some(node) = stack.pop() {
        if let Some(&(_, sublang)) = injections.iter().find(|(kind, _)| *kind == node.kind()) {
            let byte_off = node.start_byte();
            let row_off = node.start_position().row;
            for mut c in find_comments(&source[byte_off..node.end_byte()], sublang)? {
                c.start_byte += byte_off;
                c.end_byte += byte_off;
                c.start_row += row_off;
                c.end_row += row_off;
                if let Some(s) = &mut c.enclosed_by {
                    s.start_byte += byte_off;
                    s.end_byte += byte_off;
                    s.start_row += row_off;
                    s.end_row += row_off;
                }
                out.push(c);
            }
            continue;
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    Ok(())
}

fn comment_kind_from_text(text: &str) -> CommentKind {
    if text.trim_start().starts_with("/*") {
        CommentKind::Block
    } else {
        CommentKind::Line
    }
}

fn in_lines(comment: &Comment, lines: &Lines) -> bool {
    match lines {
        Lines::All => true,
        Lines::Ranges(ranges) => ranges
            .iter()
            .any(|&(s, e)| comment.start_row <= e && comment.end_row >= s),
    }
}

/// # Errors
/// Returns an error if comment extraction fails (grammar load or parse error).
pub fn strip(source: &str, lang: Lang, opts: &Options) -> Result<Outcome, Error> {
    let comments = find_comments(source, lang)?;
    let preserve = opts.preserve.for_file(&comments);

    let mut removing = vec![false; comments.len()];
    let mut preserved = 0usize;
    for (i, c) in comments.iter().enumerate() {
        if !in_lines(c, &opts.lines) || !opts.kinds.allows(c.kind) {
            continue;
        }
        if preserve.should_preserve(c) {
            preserved += 1;
            continue;
        }
        removing[i] = true;
    }
    let removed = removing.iter().filter(|&&r| r).count();

    let survivors: HashSet<Span> = comments
        .iter()
        .zip(&removing)
        .filter(|(_, &r)| !r)
        .filter_map(|(c, _)| c.enclosed_by)
        .collect();

    let mut spans: Vec<Removal> = comments
        .iter()
        .zip(&removing)
        .filter(|(_, &r)| r)
        .map(
            |(c, _)| match c.enclosed_by.filter(|w| !survivors.contains(w)) {
                Some(wrapper) => Removal::wrapping(wrapper),
                None => Removal::bare(c.span()),
            },
        )
        .collect();
    spans.dedup();

    let spans = coalesce_same_line(source, &spans);
    let output = rewrite(source, &spans, opts.line_mode);
    Ok(Outcome {
        output,
        removed,
        preserved,
    })
}
