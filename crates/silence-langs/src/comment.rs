#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CommentProfile {
    LineBlock,
    Unified,
    UnifiedMultiline,
}

impl CommentProfile {
    #[must_use]
    pub fn query(self) -> &'static str {
        match self {
            Self::LineBlock => "(line_comment) @line (block_comment) @block",
            Self::Unified => "(comment) @comment",
            Self::UnifiedMultiline => "(comment) @comment (multiline_comment) @block",
        }
    }

    #[must_use]
    pub fn capture_names(self) -> &'static [&'static str] {
        match self {
            Self::LineBlock => &["line", "block"],
            Self::Unified => &["comment"],
            Self::UnifiedMultiline => &["comment", "block"],
        }
    }
}
