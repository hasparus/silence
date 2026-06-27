use super::comment::CommentProfile;
use super::Lang;

#[derive(Clone, Copy)]
pub struct PackFields {
    pub id: &'static str,
    pub crate_name: &'static str,
    pub symbol: &'static str,
}

#[derive(Clone, Copy)]
pub struct LangSpec {
    pub lang: Lang,
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub comment: CommentProfile,
    pub pack: Option<PackFields>,
}

pub const LANGS: &[LangSpec] = &[
    LangSpec {
        lang: Lang::TypeScript,
        name: "TypeScript",
        extensions: &["ts", "mts", "cts"],
        comment: CommentProfile::Unified,
        pack: None,
    },
    LangSpec {
        lang: Lang::Tsx,
        name: "TSX",
        extensions: &["tsx"],
        comment: CommentProfile::Unified,
        pack: None,
    },
    LangSpec {
        lang: Lang::JavaScript,
        name: "JavaScript",
        extensions: &["js", "mjs", "cjs", "jsx"],
        comment: CommentProfile::Unified,
        pack: None,
    },
    LangSpec {
        lang: Lang::Python,
        name: "Python",
        extensions: &["py", "pyi"],
        comment: CommentProfile::Unified,
        pack: None,
    },
    LangSpec {
        lang: Lang::Rust,
        name: "Rust",
        extensions: &["rs"],
        comment: CommentProfile::LineBlock,
        pack: Some(PackFields {
            id: "rust",
            crate_name: "tree-sitter-rust",
            symbol: "tree_sitter_rust",
        }),
    },
    LangSpec {
        lang: Lang::Go,
        name: "Go",
        extensions: &["go"],
        comment: CommentProfile::Unified,
        pack: Some(PackFields {
            id: "go",
            crate_name: "tree-sitter-go",
            symbol: "tree_sitter_go",
        }),
    },
    LangSpec {
        lang: Lang::Toml,
        name: "TOML",
        extensions: &["toml"],
        comment: CommentProfile::Unified,
        pack: Some(PackFields {
            id: "toml",
            crate_name: "tree-sitter-toml-ng",
            symbol: "tree_sitter_toml",
        }),
    },
    LangSpec {
        lang: Lang::Cpp,
        name: "C/C++",
        extensions: &["c", "h", "cpp", "cc", "cxx", "hpp", "hh", "hxx"],
        comment: CommentProfile::Unified,
        pack: Some(PackFields {
            id: "cpp",
            crate_name: "tree-sitter-cpp",
            symbol: "tree_sitter_cpp",
        }),
    },
    LangSpec {
        lang: Lang::Java,
        name: "Java",
        extensions: &["java"],
        comment: CommentProfile::LineBlock,
        pack: Some(PackFields {
            id: "java",
            crate_name: "tree-sitter-java",
            symbol: "tree_sitter_java",
        }),
    },
    LangSpec {
        lang: Lang::Kotlin,
        name: "Kotlin",
        extensions: &["kt", "kts"],
        comment: CommentProfile::LineBlock,
        pack: Some(PackFields {
            id: "kotlin",
            crate_name: "tree-sitter-kotlin-ng",
            symbol: "tree_sitter_kotlin",
        }),
    },
    LangSpec {
        lang: Lang::Swift,
        name: "Swift",
        extensions: &["swift"],
        comment: CommentProfile::UnifiedMultiline,
        pack: Some(PackFields {
            id: "swift",
            crate_name: "tree-sitter-swift",
            symbol: "tree_sitter_swift",
        }),
    },
    LangSpec {
        lang: Lang::CSharp,
        name: "C#",
        extensions: &["cs"],
        comment: CommentProfile::Unified,
        pack: Some(PackFields {
            id: "csharp",
            crate_name: "tree-sitter-c-sharp",
            symbol: "tree_sitter_c_sharp",
        }),
    },
    LangSpec {
        lang: Lang::Css,
        name: "CSS",
        extensions: &["css"],
        comment: CommentProfile::Unified,
        pack: Some(PackFields {
            id: "css",
            crate_name: "tree-sitter-css",
            symbol: "tree_sitter_css",
        }),
    },
    LangSpec {
        lang: Lang::Json,
        name: "JSON",
        extensions: &["json", "jsonc"],
        comment: CommentProfile::Unified,
        pack: None,
    },
    LangSpec {
        lang: Lang::Yaml,
        name: "YAML",
        extensions: &["yml", "yaml"],
        comment: CommentProfile::Unified,
        pack: Some(PackFields {
            id: "yaml",
            crate_name: "tree-sitter-yaml",
            symbol: "tree_sitter_yaml",
        }),
    },
];

pub const OPTIONAL_PACK_COUNT: usize = count_optional(LANGS);

const fn count_optional(specs: &[LangSpec]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < specs.len() {
        if specs[i].pack.is_some() {
            n += 1;
        }
        i += 1;
    }
    n
}

pub fn get(lang: Lang) -> &'static LangSpec {
    LANGS
        .iter()
        .find(|spec| spec.lang == lang)
        .expect("every Lang variant has a LangSpec")
}

pub fn from_extension(ext: &str) -> Option<Lang> {
    let e = ext.to_ascii_lowercase();
    LANGS
        .iter()
        .find(|spec| spec.extensions.contains(&e.as_str()))
        .map(|spec| spec.lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_ids_are_unique() {
        let mut ids = LANGS
            .iter()
            .filter_map(|spec| spec.pack.map(|pack| pack.id))
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), OPTIONAL_PACK_COUNT);
    }

    #[test]
    fn extension_keys_are_unique() {
        let mut extensions = LANGS
            .iter()
            .flat_map(|spec| spec.extensions.iter().copied())
            .collect::<Vec<_>>();
        extensions.sort_unstable();
        let original_len = extensions.len();
        extensions.dedup();
        assert_eq!(extensions.len(), original_len);
    }

    #[test]
    fn every_spec_is_builtin_xor_optional() {
        for spec in LANGS {
            let is_builtin_lang = matches!(
                spec.lang,
                Lang::TypeScript | Lang::Tsx | Lang::JavaScript | Lang::Python | Lang::Json
            );
            assert_eq!(spec.pack.is_none(), is_builtin_lang, "{:?}", spec.lang);
        }
    }
}
