use tree_sitter::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
    Go,
    Toml,
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
            _ => return None,
        })
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
        }
    }

    #[must_use]
    pub fn grammar(self) -> Language {
        match self {
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Tsx | Lang::JavaScript => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
            Lang::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
        }
    }

    #[must_use]
    pub fn comment_query(self) -> &'static str {
        match self {
            Lang::Rust => "(line_comment) @comment (block_comment) @comment",
            Lang::TypeScript
            | Lang::Tsx
            | Lang::JavaScript
            | Lang::Python
            | Lang::Go
            | Lang::Toml => "(comment) @comment",
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
        assert_eq!(Lang::from_extension("unknown"), None);
    }

    #[test]
    fn every_grammar_loads_and_query_compiles() {
        for &lang in ALL {
            let grammar = lang.grammar();
            let query = tree_sitter::Query::new(&grammar, lang.comment_query())
                .unwrap_or_else(|e| panic!("query failed for {}: {e:?}", lang.name()));
            assert!(
                query.capture_names().contains(&"comment"),
                "{} query must define @comment",
                lang.name()
            );
        }
    }
}
