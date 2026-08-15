use silence_langs::Lang;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

mod preserve;
pub use preserve::{PreserveConfig, DEFAULT_PRESERVE_PATTERNS};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_row: usize,
    pub end_row: usize,
    pub text: String,
    pub kind: CommentKind,
}

impl Comment {
    #[must_use]
    pub fn is_multiline(&self) -> bool {
        self.end_row > self.start_row
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

    let mut to_remove: Vec<&Comment> = Vec::new();
    let mut preserved = 0usize;
    for c in &comments {
        if !in_lines(c, &opts.lines) {
            continue;
        }
        if !opts.kinds.allows(c.kind) {
            continue;
        }
        if preserve.should_preserve(c) {
            preserved += 1;
            continue;
        }
        to_remove.push(c);
    }

    let removed = to_remove.len();
    let spans = coalesce_same_line(source, &to_remove);
    let output = rewrite(source, &spans, opts.line_mode);
    Ok(Outcome {
        output,
        removed,
        preserved,
    })
}

fn coalesce_same_line(source: &str, comments: &[&Comment]) -> Vec<Comment> {
    let mut out: Vec<Comment> = Vec::with_capacity(comments.len());
    for &c in comments {
        if let Some(last) = out.last_mut() {
            if c.start_byte >= last.end_byte {
                let gap = &source[last.end_byte..c.start_byte];
                if gap.bytes().all(|b| b == b' ' || b == b'\t') {
                    last.end_byte = c.end_byte;
                    last.end_row = c.end_row;
                    last.text.push_str(gap);
                    last.text.push_str(&c.text);
                    continue;
                }
            }
        }
        out.push(c.clone());
    }
    out
}

fn separator_needed(left: &str, right: &str) -> bool {
    fn is_word(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }
    matches!(
        (left.chars().next_back(), right.chars().next()),
        (Some(p), Some(n)) if is_word(p) && is_word(n)
    )
}

fn rewrite(source: &str, remove: &[Comment], mode: LineMode) -> String {
    if remove.is_empty() {
        return source.to_string();
    }

    let bytes = source.as_bytes();
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut result = String::with_capacity(source.len());
    let mut cursor = 0usize;

    for c in remove {
        if c.start_byte < cursor {
            continue;
        }
        result.push_str(&source[cursor..c.start_byte]);

        let line_start = source[..c.start_byte].rfind('\n').map_or(0, |i| i + 1);
        let before = &source[line_start..c.start_byte];
        let prefix_blank = before.trim().is_empty();

        let line_end = source[c.end_byte..]
            .find('\n')
            .map_or(bytes.len(), |i| c.end_byte + i);
        let after = &source[c.end_byte..line_end];
        let suffix_blank = after.trim().is_empty();

        let line_only = prefix_blank && suffix_blank;

        if line_only {
            match mode {
                LineMode::Collapse => {
                    result.truncate(result.len() - before.len());
                    cursor = if line_end < bytes.len() {
                        line_end + 1
                    } else {
                        line_end
                    };
                }
                LineMode::PreserveLines => {
                    result.truncate(result.len() - before.len());
                    for _ in 0..(c.end_row - c.start_row) {
                        result.push_str(newline);
                    }
                    cursor = c.end_byte;
                }
            }
        } else {
            let trimmed_len = result.trim_end_matches([' ', '\t']).len();
            result.truncate(trimmed_len);
            cursor = c.end_byte;
            if suffix_blank {
                cursor = line_end;
                if line_end > c.end_byte && bytes[line_end - 1] == b'\r' {
                    cursor = line_end - 1;
                }
            } else if separator_needed(&result, &source[cursor..]) {
                result.push(' ');
            }
        }
    }

    result.push_str(&source[cursor..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use silence_langs::Lang;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn strip_default(src: &str, lang: Lang) -> Result<String, Error> {
        Ok(strip(src, lang, &Options::default())?.output)
    }

    /// The pair is what protects the pair. A sentinel with no partner in the
    /// file is indistinguishable from prose, so it is treated as prose.
    #[test]
    fn only_a_sentinel_with_its_partner_survives() -> TestResult {
        let src = "let a = 1; // codegen-start\nlet b = 2; // slop\nlet c = 3; // codegen-end\n";
        let out = strip_default(src, Lang::Rust)?;
        assert!(out.contains("codegen-start"), "{out}");
        assert!(out.contains("codegen-end"), "{out}");
        assert!(!out.contains("slop"), "{out}");

        let lone = strip_default("let a = 1; // codegen-end\n", Lang::Rust)?;
        assert_eq!(lone, "let a = 1;\n");
        Ok(())
    }

    /// C and Java put a banner fence on its own line behind a `*` gutter, so
    /// the sentinel is not the first token of the comment.
    #[test]
    fn a_gutter_does_not_hide_the_sentinel() -> TestResult {
        let src =
            "/*\n * codegen-start abc\n */\nlet x = 1; // slop\n/*\n * codegen-end abc\n */\n";
        let out = strip_default(src, Lang::Rust)?;
        assert!(out.contains("codegen-start abc"), "{out}");
        assert!(out.contains("codegen-end abc"), "{out}");
        assert!(!out.contains("slop"), "{out}");
        Ok(())
    }

    /// A pair fences the region between its halves. Halves in the wrong order
    /// fence nothing, so they are a coincidence of wording, not a marker.
    #[test]
    fn a_close_before_its_open_is_not_a_pair() -> TestResult {
        let src = "let a = 1; // zeta-end came first\nlet b = 2; // zeta-start came second\n";
        assert_eq!(strip_default(src, Lang::Rust)?, "let a = 1;\nlet b = 2;\n");
        Ok(())
    }

    /// front-end, cold-start and friends have a sentinel's exact shape. Nothing
    /// but the missing partner separates them from a real marker.
    #[test]
    fn hyphenated_compounds_are_still_prose() -> TestResult {
        let src = "let a = 1; // front-end only\nlet b = 2; // cold-start path\nlet c = 3; // dead-end\nlet d = 4; // Back-End\n";
        let out = strip_default(src, Lang::Rust)?;
        assert_eq!(out, "let a = 1;\nlet b = 2;\nlet c = 3;\nlet d = 4;\n");
        Ok(())
    }

    #[test]
    fn markers_go_away_with_directive_detection_off() -> TestResult {
        let src = "let a = 1; // codegen-start\nlet b = 2; // codegen-end\n";
        let opts = Options {
            preserve: PreserveConfig::empty(),
            ..Default::default()
        };
        assert_eq!(
            strip(src, Lang::Rust, &opts)?.output,
            "let a = 1;\nlet b = 2;\n"
        );
        Ok(())
    }

    #[test]
    fn jsx_machine_markers_survive_while_prose_goes() -> TestResult {
        let src = "<div>\n      {/* impeccable-variants-start cd383158 */}\n      {/* the card */}\n      <Card />\n      {/* impeccable-variants-end cd383158 */}\n    </div>\n";
        let out = strip_default(src, Lang::Tsx)?;
        assert!(out.contains("impeccable-variants-start cd383158"));
        assert!(out.contains("impeccable-variants-end cd383158"));
        assert!(!out.contains("the card"));
        Ok(())
    }

    #[test]
    fn comment_marker_inside_string_is_not_removed() -> TestResult {
        let src = r#"let url = "http://example.com"; // strip me"#;
        let out = strip_default(src, Lang::Rust)?;
        assert_eq!(out, r#"let url = "http://example.com";"#);
        Ok(())
    }

    #[test]
    fn hash_inside_python_string_is_safe() -> TestResult {
        let src = r#"pattern = "a#b"  # real comment"#;
        let out = strip_default(src, Lang::Python)?;
        assert_eq!(out, r#"pattern = "a#b""#);
        Ok(())
    }

    #[test]
    fn unbalanced_quote_does_not_desync() -> TestResult {
        let src = "let x = 5; // it's fine\nlet y = 6;";
        let out = strip_default(src, Lang::Rust)?;
        assert_eq!(out, "let x = 5;\nlet y = 6;");
        Ok(())
    }

    #[test]
    fn multiline_block_comment_fully_removed() -> TestResult {
        let src = "fn a() {}\n/* line one\n   line two\n   line three */\nfn b() {}\n";
        let out = strip_default(src, Lang::Rust)?;
        assert_eq!(out, "fn a() {}\nfn b() {}\n");
        Ok(())
    }

    #[test]
    fn string_with_trailing_backslash_then_comment() -> TestResult {
        let src = r#"let p = "foo\\"; // comment"#;
        let out = strip_default(src, Lang::Rust)?;
        assert_eq!(out, r#"let p = "foo\\";"#);
        Ok(())
    }

    #[test]
    fn trailing_comment_trims_whitespace() -> TestResult {
        let src = "let x = 5;    // foo\n";
        assert_eq!(strip_default(src, Lang::Rust)?, "let x = 5;\n");
        Ok(())
    }

    #[test]
    fn comment_only_line_is_deleted_in_collapse_mode() -> TestResult {
        let src = "fn a() {}\n// gone\nfn b() {}\n";
        assert_eq!(strip_default(src, Lang::Rust)?, "fn a() {}\nfn b() {}\n");
        Ok(())
    }

    #[test]
    fn comment_only_line_blanked_in_preserve_mode() -> TestResult {
        let src = "fn a() {}\n// gone\nfn b() {}\n";
        let opts = Options {
            line_mode: LineMode::PreserveLines,
            ..Default::default()
        };
        let out = strip(src, Lang::Rust, &opts)?.output;
        assert_eq!(out, "fn a() {}\n\nfn b() {}\n");
        Ok(())
    }

    #[test]
    fn preserve_pattern_keeps_todo() -> TestResult {
        let src = "// TODO: keep me\n// remove me\nfn a() {}\n";
        let opts = Options {
            preserve: PreserveConfig::with_patterns(vec!["TODO".into()]),
            ..Default::default()
        };
        let outcome = strip(src, Lang::Rust, &opts)?;
        assert_eq!(outcome.output, "// TODO: keep me\nfn a() {}\n");
        assert_eq!(outcome.removed, 1);
        assert_eq!(outcome.preserved, 1);
        Ok(())
    }

    #[test]
    fn default_preserves_doc_comments() -> TestResult {
        let src = "/// Public docs.\nfunction a() {}\n/** JSDoc. */\nconst x = 1; // strip\n";
        let out = strip_default(src, Lang::JavaScript)?;
        assert_eq!(
            out,
            "/// Public docs.\nfunction a() {}\n/** JSDoc. */\nconst x = 1;\n"
        );
        Ok(())
    }

    #[test]
    fn line_ranges_limit_scope() -> TestResult {
        let src = "// keep\nlet x = 1;\nlet y = 2; // go\n";
        let opts = Options {
            lines: Lines::Ranges(vec![(3, 3)]),
            ..Default::default()
        };
        let out = strip(src, Lang::Rust, &opts)?.output;
        assert_eq!(out, "// keep\nlet x = 1;\nlet y = 2;\n");
        Ok(())
    }

    #[test]
    fn inline_filter_keeps_block_comments() -> TestResult {
        let src = "// line\n/* block */\nfn a() {}\n";
        let opts = Options {
            kinds: CommentKinds {
                line: true,
                block: false,
            },
            ..Default::default()
        };
        assert_eq!(
            strip(src, Lang::Rust, &opts)?.output,
            "/* block */\nfn a() {}\n"
        );
        Ok(())
    }

    #[test]
    fn block_filter_keeps_line_comments() -> TestResult {
        let src = "// line\n/* block */\nfn a() {}\n";
        let opts = Options {
            kinds: CommentKinds {
                line: false,
                block: true,
            },
            ..Default::default()
        };
        assert_eq!(
            strip(src, Lang::Rust, &opts)?.output,
            "// line\nfn a() {}\n"
        );
        Ok(())
    }

    #[test]
    fn go_and_python_and_ts_smoke() -> TestResult {
        assert_eq!(
            strip_default("package main\n// x\nfunc main() {}\n", Lang::Go)?,
            "package main\nfunc main() {}\n"
        );
        assert_eq!(strip_default("x = 1  # c\n", Lang::Python)?, "x = 1\n");
        assert_eq!(
            strip_default("const x = 1; // c\n", Lang::TypeScript)?,
            "const x = 1;\n"
        );
        Ok(())
    }

    #[test]
    fn optional_pack_strip_smoke() -> TestResult {
        const CASES: &[(Lang, &str, &str)] = &[
            (
                Lang::Java,
                "class Main {\n  // x\n  void f() {}\n}\n",
                "class Main {\n  void f() {}\n}\n",
            ),
            (
                Lang::Kotlin,
                "fun main() {\n  // x\n  println(\"hi\")\n}\n",
                "fun main() {\n  println(\"hi\")\n}\n",
            ),
            (
                Lang::CSharp,
                "class Main {\n  // x\n  void F() {}\n}\n",
                "class Main {\n  void F() {}\n}\n",
            ),
            (
                Lang::Swift,
                "func main() {\n  // x\n  print(\"hi\")\n}\n",
                "func main() {\n  print(\"hi\")\n}\n",
            ),
            (
                Lang::Css,
                ".a {\n  /* x */\n  color: red;\n}\n",
                ".a {\n  color: red;\n}\n",
            ),
            (
                Lang::Json,
                "{\n  // x\n  \"a\": 1\n}\n",
                "{\n  \"a\": 1\n}\n",
            ),
            (Lang::Yaml, "a: 1\n# x\nb: 2\n", "a: 1\nb: 2\n"),
        ];
        for &(lang, src, expected) in CASES {
            assert_eq!(strip_default(src, lang)?, expected);
        }
        Ok(())
    }

    #[test]
    fn css_inline_block_comment_trims_to_declaration() -> TestResult {
        let src = ".a { color: red; /* strip me */ }\n";
        assert_eq!(strip_default(src, Lang::Css)?, ".a { color: red; }\n");
        Ok(())
    }

    #[test]
    fn css_url_with_comment_marker_is_safe() -> TestResult {
        let src = ".a {\n  background: url(\"http://example.com/*x*/\");\n  /* gone */\n}\n";
        assert_eq!(
            strip_default(src, Lang::Css)?,
            ".a {\n  background: url(\"http://example.com/*x*/\");\n}\n"
        );
        Ok(())
    }

    #[test]
    fn css_multiline_block_comment_fully_removed() -> TestResult {
        let src = ".a {}\n/* line one\n   line two */\n.b {}\n";
        assert_eq!(strip_default(src, Lang::Css)?, ".a {}\n.b {}\n");
        Ok(())
    }

    #[test]
    fn python_shebang_is_preserved() -> TestResult {
        let src = "#!/usr/bin/env python3\n# remove me\nx = 1\n";
        assert_eq!(
            strip_default(src, Lang::Python)?,
            "#!/usr/bin/env python3\nx = 1\n"
        );
        Ok(())
    }

    #[test]
    fn multiple_block_comments_on_one_line_collapse() -> TestResult {
        let src = "fn a() {}\n/* one */ /* two */\nfn b() {}\n";
        assert_eq!(strip_default(src, Lang::Rust)?, "fn a() {}\nfn b() {}\n");
        Ok(())
    }

    #[test]
    fn inline_block_comment_keeps_token_separator() -> TestResult {
        let src = "let/* hi */x = 5;\n";
        assert_eq!(strip_default(src, Lang::Rust)?, "let x = 5;\n");
        Ok(())
    }

    #[test]
    fn crlf_trailing_comment_keeps_carriage_return() -> TestResult {
        let src = "let x = 5; // c\r\nlet y = 6;\r\n";
        assert_eq!(
            strip_default(src, Lang::Rust)?,
            "let x = 5;\r\nlet y = 6;\r\n"
        );
        Ok(())
    }

    #[test]
    fn astro_strips_frontmatter_and_template_comments() -> TestResult {
        let src = "---\nconst x = 1; // strip\n// gone\n---\n<div>hi</div>\n<!-- strip -->\n";
        assert_eq!(
            strip_default(src, Lang::Astro)?,
            "---\nconst x = 1;\n---\n<div>hi</div>\n"
        );
        Ok(())
    }

    #[test]
    fn astro_template_only_strips_html_comments() -> TestResult {
        let src = "<div>hi</div>\n<!-- gone -->\n";
        assert_eq!(strip_default(src, Lang::Astro)?, "<div>hi</div>\n");
        Ok(())
    }

    #[test]
    fn astro_line_ranges_use_file_rows() -> TestResult {
        let src = "---\nlet a = 1; // keep\nlet b = 2; // go\n---\n";
        let opts = Options {
            lines: Lines::Ranges(vec![(3, 3)]),
            ..Default::default()
        };
        assert_eq!(
            strip(src, Lang::Astro, &opts)?.output,
            "---\nlet a = 1; // keep\nlet b = 2;\n---\n"
        );
        Ok(())
    }

    #[test]
    fn jsx_comment_in_element_is_removed() -> TestResult {
        let src = "const a = <div>{/* note */}</div>;\n";
        assert_eq!(
            strip_default(src, Lang::JavaScript)?,
            "const a = <div>{}</div>;\n"
        );
        Ok(())
    }
}
