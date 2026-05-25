#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
    Go,
    Toml,
    Cpp,
}

impl Lang {
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Lang> {
        let e = ext.to_ascii_lowercase();
        Some(match e.as_str() {
            "rs" => Lang::Rust,
            "ts" | "mts" | "cts" => Lang::TypeScript,
            "tsx" => Lang::Tsx,
            "js" | "mjs" | "cjs" | "jsx" => Lang::JavaScript,
            "py" | "pyi" => Lang::Python,
            "go" => Lang::Go,
            "toml" => Lang::Toml,
            "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Lang::Cpp,
            _ => return None,
        })
    }

    #[must_use]
    pub fn is_builtin(self) -> bool {
        matches!(
            self,
            Lang::TypeScript | Lang::Tsx | Lang::JavaScript | Lang::Python
        )
    }

    #[must_use]
    pub fn grammar_pack_id(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::Go => "go",
            Lang::Toml => "toml",
            Lang::Cpp => "cpp",
            Lang::TypeScript | Lang::Tsx | Lang::JavaScript | Lang::Python => {
                panic!("built-in language has no grammar pack id")
            }
        }
    }

    #[must_use]
    pub fn grammar_symbol(self) -> &'static str {
        match self {
            Lang::Rust => "tree_sitter_rust",
            Lang::Go => "tree_sitter_go",
            Lang::Toml => "tree_sitter_toml",
            Lang::Cpp => "tree_sitter_cpp",
            Lang::TypeScript | Lang::Tsx | Lang::JavaScript | Lang::Python => {
                panic!("built-in language has no dylib symbol")
            }
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Lang::Rust => "Rust",
            Lang::TypeScript => "TypeScript",
            Lang::Tsx => "TSX",
            Lang::JavaScript => "JavaScript",
            Lang::Python => "Python",
            Lang::Go => "Go",
            Lang::Toml => "TOML",
            Lang::Cpp => "C/C++",
        }
    }

    #[must_use]
    pub fn comment_query(self) -> &'static str {
        match self {
            Lang::Rust => "(line_comment) @line (block_comment) @block",
            Lang::TypeScript
            | Lang::Tsx
            | Lang::JavaScript
            | Lang::Python
            | Lang::Go
            | Lang::Toml
            | Lang::Cpp => "(comment) @comment",
        }
    }

    #[must_use]
    pub fn comment_capture_names(self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &["line", "block"],
            Lang::TypeScript
            | Lang::Tsx
            | Lang::JavaScript
            | Lang::Python
            | Lang::Go
            | Lang::Toml
            | Lang::Cpp => &["comment"],
        }
    }
}

pub const ALL: &[Lang] = &[
    Lang::Rust,
    Lang::TypeScript,
    Lang::Tsx,
    Lang::JavaScript,
    Lang::Python,
    Lang::Go,
    Lang::Toml,
    Lang::Cpp,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_resolution() {
        assert_eq!(Lang::from_extension("rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_extension("RS"), Some(Lang::Rust));
        assert_eq!(Lang::from_extension("tsx"), Some(Lang::Tsx));
        assert_eq!(Lang::from_extension("py"), Some(Lang::Python));
        assert_eq!(Lang::from_extension("ts"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_extension("c"), Some(Lang::Cpp));
        assert_eq!(Lang::from_extension("cpp"), Some(Lang::Cpp));
        assert_eq!(Lang::from_extension("unknown"), None);
    }

    #[test]
    fn builtin_vs_optional() {
        assert!(Lang::Python.is_builtin());
        assert!(Lang::TypeScript.is_builtin());
        assert!(!Lang::Rust.is_builtin());
        assert!(!Lang::Cpp.is_builtin());
    }
}
