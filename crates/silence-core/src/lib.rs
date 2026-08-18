use silence_langs::Lang;
use std::collections::HashSet;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

mod jsx;
mod preserve;
mod rewrite;
use jsx::Container;
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
    /// The `{ }` of a JSX expression this comment is the whole content of, and
    /// where the text runs beside it reach. A fact about the parse, carrying no
    /// claim that the braces may go — [`strip`] settles that once it knows
    /// which comments are actually leaving.
    pub(crate) enclosed_by: Option<Container>,
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
                enclosed_by: jsx::container(node, source, lang),
            });
        }
    }

    collect_injected(source, tree.root_node(), lang.injections(), &mut out)?;

    out.sort_by_key(|c| c.start_byte);
    Ok(out)
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
                // Dead while the only injection is Astro's TypeScript
                // frontmatter, which has no JSX. Correct the day one does.
                if let Some(w) = &mut c.enclosed_by {
                    w.shift(byte_off, row_off);
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

    let mut going: Vec<&Comment> = Vec::new();
    let mut staying: Vec<&Comment> = Vec::new();
    let mut preserved = 0usize;
    for c in &comments {
        if !in_lines(c, &opts.lines) || !opts.kinds.allows(c.kind) {
            staying.push(c);
            continue;
        }
        if preserve.should_preserve(c) {
            preserved += 1;
            staying.push(c);
            continue;
        }
        going.push(c);
    }
    let removed = going.len();

    let takeable = takeable_containers(source, &going, &staying);

    let mut spans: Vec<Removal> = going
        .iter()
        .map(|c| match c.enclosed_by.filter(|w| takeable.contains(w)) {
            Some(container) => Removal::wrapping(container.span),
            None => Removal::bare(c.span()),
        })
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

/// Which containers may lose their braces. Decided here rather than at parse
/// time because it depends on which comments are leaving: a container whose
/// comment stays keeps its braces, and holds the text either side of it apart.
///
/// The rest are judged in adjacent runs. Removing one container joins the text
/// beside it to its neighbour's, so a run of them stands or falls whole — and
/// judging a run that includes a container which is staying would prove an
/// identity nobody relies on.
fn takeable_containers(
    source: &str,
    going: &[&Comment],
    staying: &[&Comment],
) -> HashSet<Container> {
    let held: HashSet<Container> = staying.iter().filter_map(|c| c.enclosed_by).collect();

    let mut open: Vec<Container> = going
        .iter()
        .filter_map(|c| c.enclosed_by)
        .filter(|w| !held.contains(w))
        .collect();
    open.dedup();

    let mut takeable = HashSet::new();
    let mut start = 0;
    while start < open.len() {
        let mut end = start + 1;
        while end < open.len() && open[end - 1].adjoins(open[end]) {
            end += 1;
        }
        if jsx::taking_braces_is_invisible(source, &open[start..end]) {
            takeable.extend(&open[start..end]);
        }
        start = end;
    }
    takeable
}
