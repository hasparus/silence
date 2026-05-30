use super::Lang;

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

pub struct OptionalPack {
    pub lang: Lang,
    pub id: &'static str,
    pub crate_name: &'static str,
    pub symbol: &'static str,
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub comment: CommentProfile,
}

pub const PACKS: &[OptionalPack] = &[
    OptionalPack {
        lang: Lang::Rust,
        id: "rust",
        crate_name: "tree-sitter-rust",
        symbol: "tree_sitter_rust",
        name: "Rust",
        extensions: &["rs"],
        comment: CommentProfile::LineBlock,
    },
    OptionalPack {
        lang: Lang::Go,
        id: "go",
        crate_name: "tree-sitter-go",
        symbol: "tree_sitter_go",
        name: "Go",
        extensions: &["go"],
        comment: CommentProfile::Unified,
    },
    OptionalPack {
        lang: Lang::Toml,
        id: "toml",
        crate_name: "tree-sitter-toml-ng",
        symbol: "tree_sitter_toml",
        name: "TOML",
        extensions: &["toml"],
        comment: CommentProfile::Unified,
    },
    OptionalPack {
        lang: Lang::Cpp,
        id: "cpp",
        crate_name: "tree-sitter-cpp",
        symbol: "tree_sitter_cpp",
        name: "C/C++",
        extensions: &["c", "h", "cpp", "cc", "cxx", "hpp", "hh", "hxx"],
        comment: CommentProfile::Unified,
    },
    OptionalPack {
        lang: Lang::Java,
        id: "java",
        crate_name: "tree-sitter-java",
        symbol: "tree_sitter_java",
        name: "Java",
        extensions: &["java"],
        comment: CommentProfile::LineBlock,
    },
    OptionalPack {
        lang: Lang::Kotlin,
        id: "kotlin",
        crate_name: "tree-sitter-kotlin-ng",
        symbol: "tree_sitter_kotlin",
        name: "Kotlin",
        extensions: &["kt", "kts"],
        comment: CommentProfile::LineBlock,
    },
    OptionalPack {
        lang: Lang::Swift,
        id: "swift",
        crate_name: "tree-sitter-swift",
        symbol: "tree_sitter_swift",
        name: "Swift",
        extensions: &["swift"],
        comment: CommentProfile::UnifiedMultiline,
    },
    OptionalPack {
        lang: Lang::CSharp,
        id: "csharp",
        crate_name: "tree-sitter-c-sharp",
        symbol: "tree_sitter_c_sharp",
        name: "C#",
        extensions: &["cs"],
        comment: CommentProfile::Unified,
    },
];

pub fn from_extension(ext: &str) -> Option<Lang> {
    let e = ext.to_ascii_lowercase();
    PACKS
        .iter()
        .find(|pack| pack.extensions.contains(&e.as_str()))
        .map(|pack| pack.lang)
}

pub fn get(lang: Lang) -> Option<&'static OptionalPack> {
    PACKS.iter().find(|pack| pack.lang == lang)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::BUILTINS;

    #[test]
    fn packs_do_not_include_builtins() {
        for pack in PACKS {
            assert!(
                !pack.lang.is_builtin(),
                "{:?} must not be in PACKS",
                pack.lang
            );
        }
    }

    #[test]
    fn extension_keys_are_unique() {
        let mut extensions = BUILTINS
            .iter()
            .flat_map(|b| b.extensions.iter().copied())
            .chain(
                PACKS
                    .iter()
                    .flat_map(|pack| pack.extensions.iter().copied()),
            )
            .collect::<Vec<_>>();
        extensions.sort_unstable();
        let original_len = extensions.len();
        extensions.dedup();
        assert_eq!(extensions.len(), original_len);
    }
}
