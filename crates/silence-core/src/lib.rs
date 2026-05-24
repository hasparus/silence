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

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub line_mode: LineMode,
    pub preserve: PreserveConfig,
    pub line_ranges: Vec<(usize, usize)>,
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

pub fn find_comments(source: &str, lang: Lang) -> Result<Vec<Comment>, Error> {
    let grammar = lang.grammar();
    let mut parser = Parser::new();
    parser
        .set_language(&grammar)
        .map_err(|e| Error::Language(e.to_string()))?;
    let tree = parser.parse(source, None).ok_or(Error::Parse)?;

    let query =
        Query::new(&grammar, lang.comment_query()).map_err(|e| Error::Query(e.to_string()))?;
    let comment_idx = query
        .capture_index_for_name("comment")
        .ok_or_else(|| Error::Query("missing @comment capture".into()))?;

    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();
    let mut out = Vec::new();

    let mut matches = cursor.matches(&query, tree.root_node(), bytes);
    while let Some(m) = matches.next() {
        for cap in m.captures {
            if cap.index != comment_idx {
                continue;
            }
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

            let kind = if text.trim_start().starts_with("/*") {
                CommentKind::Block
            } else {
                CommentKind::Line
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

    out.sort_by_key(|c| c.start_byte);
    Ok(out)
}

fn in_ranges(comment: &Comment, ranges: &[(usize, usize)]) -> bool {
    if ranges.is_empty() {
        return true;
    }
    ranges
        .iter()
        .any(|&(s, e)| comment.start_row <= e && comment.end_row >= s)
}

pub fn strip(source: &str, lang: Lang, opts: &Options) -> Result<Outcome, Error> {
    let comments = find_comments(source, lang)?;

    let mut to_remove: Vec<&Comment> = Vec::new();
    let mut preserved = 0usize;
    for c in &comments {
        if !in_ranges(c, &opts.line_ranges) {
            continue;
        }
        if !opts.kinds.allows(c.kind) {
            continue;
        }
        if opts.preserve.should_preserve(&c.text) {
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

    fn strip_default(src: &str, lang: Lang) -> String {
        strip(src, lang, &Options::default()).unwrap().output
    }

    #[test]
    fn comment_marker_inside_string_is_not_removed() {
        let src = r#"let url = "http://example.com"; // strip me"#;
        let out = strip_default(src, Lang::Rust);
        assert_eq!(out, r#"let url = "http://example.com";"#);
    }

    #[test]
    fn hash_inside_python_string_is_safe() {
        let src = r#"pattern = "a#b"  # real comment"#;
        let out = strip_default(src, Lang::Python);
        assert_eq!(out, r#"pattern = "a#b""#);
    }

    #[test]
    fn unbalanced_quote_does_not_desync() {
        let src = "let x = 5; // it's fine\nlet y = 6;";
        let out = strip_default(src, Lang::Rust);
        assert_eq!(out, "let x = 5;\nlet y = 6;");
    }

    #[test]
    fn multiline_block_comment_fully_removed() {
        let src = "fn a() {}\n/* line one\n   line two\n   line three */\nfn b() {}\n";
        let out = strip_default(src, Lang::Rust);
        assert_eq!(out, "fn a() {}\nfn b() {}\n");
    }

    #[test]
    fn string_with_trailing_backslash_then_comment() {
        let src = r#"let p = "foo\\"; // comment"#;
        let out = strip_default(src, Lang::Rust);
        assert_eq!(out, r#"let p = "foo\\";"#);
    }

    #[test]
    fn trailing_comment_trims_whitespace() {
        let src = "let x = 5;    // foo\n";
        assert_eq!(strip_default(src, Lang::Rust), "let x = 5;\n");
    }

    #[test]
    fn comment_only_line_is_deleted_in_collapse_mode() {
        let src = "fn a() {}\n// gone\nfn b() {}\n";
        assert_eq!(strip_default(src, Lang::Rust), "fn a() {}\nfn b() {}\n");
    }

    #[test]
    fn comment_only_line_blanked_in_preserve_mode() {
        let src = "fn a() {}\n// gone\nfn b() {}\n";
        let opts = Options {
            line_mode: LineMode::PreserveLines,
            ..Default::default()
        };
        let out = strip(src, Lang::Rust, &opts).unwrap().output;
        assert_eq!(out, "fn a() {}\n\nfn b() {}\n");
    }

    #[test]
    fn preserve_pattern_keeps_todo() {
        let src = "// TODO: keep me\n// remove me\nfn a() {}\n";
        let opts = Options {
            preserve: PreserveConfig::with_patterns(vec!["TODO".into()]),
            ..Default::default()
        };
        let outcome = strip(src, Lang::Rust, &opts).unwrap();
        assert_eq!(outcome.output, "// TODO: keep me\nfn a() {}\n");
        assert_eq!(outcome.removed, 1);
        assert_eq!(outcome.preserved, 1);
    }

    #[test]
    fn default_preserves_doc_comments() {
        let src = "/// Public docs.\nfunction a() {}\n/** JSDoc. */\nconst x = 1; // strip\n";
        let out = strip_default(src, Lang::JavaScript);
        assert_eq!(
            out,
            "/// Public docs.\nfunction a() {}\n/** JSDoc. */\nconst x = 1;\n"
        );
    }

    #[test]
    fn line_ranges_limit_scope() {
        let src = "// keep\nlet x = 1;\nlet y = 2; // go\n";
        let opts = Options {
            line_ranges: vec![(3, 3)],
            ..Default::default()
        };
        let out = strip(src, Lang::Rust, &opts).unwrap().output;
        assert_eq!(out, "// keep\nlet x = 1;\nlet y = 2;\n");
    }

    #[test]
    fn inline_filter_keeps_block_comments() {
        let src = "// line\n/* block */\nfn a() {}\n";
        let opts = Options {
            kinds: CommentKinds {
                line: true,
                block: false,
            },
            ..Default::default()
        };
        assert_eq!(
            strip(src, Lang::Rust, &opts).unwrap().output,
            "/* block */\nfn a() {}\n"
        );
    }

    #[test]
    fn block_filter_keeps_line_comments() {
        let src = "// line\n/* block */\nfn a() {}\n";
        let opts = Options {
            kinds: CommentKinds {
                line: false,
                block: true,
            },
            ..Default::default()
        };
        assert_eq!(
            strip(src, Lang::Rust, &opts).unwrap().output,
            "// line\nfn a() {}\n"
        );
    }

    #[test]
    fn go_and_python_and_ts_smoke() {
        assert_eq!(
            strip_default("package main\n// x\nfunc main() {}\n", Lang::Go),
            "package main\nfunc main() {}\n"
        );
        assert_eq!(strip_default("x = 1  # c\n", Lang::Python), "x = 1\n");
        assert_eq!(
            strip_default("const x = 1; // c\n", Lang::TypeScript),
            "const x = 1;\n"
        );
    }

    #[test]
    fn python_shebang_is_preserved() {
        let src = "#!/usr/bin/env python3\n# remove me\nx = 1\n";
        assert_eq!(
            strip_default(src, Lang::Python),
            "#!/usr/bin/env python3\nx = 1\n"
        );
    }

    #[test]
    fn multiple_block_comments_on_one_line_collapse() {
        let src = "fn a() {}\n/* one */ /* two */\nfn b() {}\n";
        assert_eq!(strip_default(src, Lang::Rust), "fn a() {}\nfn b() {}\n");
    }

    #[test]
    fn inline_block_comment_keeps_token_separator() {
        let src = "let/* hi */x = 5;\n";
        assert_eq!(strip_default(src, Lang::Rust), "let x = 5;\n");
    }

    #[test]
    fn crlf_trailing_comment_keeps_carriage_return() {
        let src = "let x = 5; // c\r\nlet y = 6;\r\n";
        assert_eq!(
            strip_default(src, Lang::Rust),
            "let x = 5;\r\nlet y = 6;\r\n"
        );
    }

    #[test]
    fn jsx_comment_in_element_is_removed() {
        let src = "const a = <div>{/* note */}</div>;\n";
        assert_eq!(
            strip_default(src, Lang::JavaScript),
            "const a = <div>{}</div>;\n"
        );
    }
}
