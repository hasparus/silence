use crate::LineMode;

/// A span of source: bytes for slicing, 1-based rows for scoping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Span {
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
    pub(crate) start_row: usize,
    pub(crate) end_row: usize,
}

/// One cut to make, and whether it takes delimiters with it. `strip` knows
/// which it is, so nothing downstream has to work it out from the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Removal {
    span: Span,
    delimited: bool,
}

impl Removal {
    /// A cut that takes the comment and nothing else.
    pub(crate) fn bare(span: Span) -> Removal {
        Removal {
            span,
            delimited: false,
        }
    }

    /// A cut over syntax that exists only to hold comments, which takes the
    /// delimiters with it. Whether that is invisible is settled before the cut
    /// is built, by the language that knows what the delimiters mean.
    pub(crate) fn wrapping(span: Span) -> Removal {
        Removal {
            span,
            delimited: true,
        }
    }
}

/// Runs of removals separated by nothing but padding, merged so the line they
/// sit on is judged once. Delimited cuts never merge: what sits between two of
/// them is content, and each answers for its own delimiters.
pub(crate) fn coalesce_same_line(source: &str, spans: &[Removal]) -> Vec<Removal> {
    let mut out: Vec<Removal> = Vec::with_capacity(spans.len());
    for &c in spans {
        if let Some(last) = out.last_mut() {
            if !last.delimited && !c.delimited && c.span.start_byte >= last.span.end_byte {
                let gap = &source[last.span.end_byte..c.span.start_byte];
                if gap.bytes().all(|b| b == b' ' || b == b'\t') {
                    last.span.end_byte = c.span.end_byte;
                    last.span.end_row = c.span.end_row;
                    continue;
                }
            }
        }
        out.push(c);
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

pub(crate) fn rewrite(source: &str, remove: &[Removal], mode: LineMode) -> String {
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

    for removal in remove {
        let (c, delimited) = (removal.span, removal.delimited);
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
        } else if suffix_blank {
            let trimmed_len = result.trim_end_matches([' ', '\t']).len();
            result.truncate(trimmed_len);
            cursor = line_end;
            if line_end > c.end_byte && bytes[line_end - 1] == b'\r' {
                cursor = line_end - 1;
            }
        } else if delimited {
            cursor = c.end_byte;
        } else {
            let trimmed_len = result.trim_end_matches([' ', '\t']).len();
            result.truncate(trimmed_len);
            cursor = c.end_byte;
            if separator_needed(&result, &source[cursor..]) {
                result.push(' ');
            }
        }
    }

    result.push_str(&source[cursor..]);
    result
}
