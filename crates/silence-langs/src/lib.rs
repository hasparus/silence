mod comment;
mod registry;

pub use comment::CommentProfile;
pub use registry::{LangSpec, PackFields, LANGS, OPTIONAL_PACK_COUNT};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    TypeScript,
    Tsx,
    JavaScript,
    Python,
    Rust,
    Go,
    Toml,
    Cpp,
    Java,
    Kotlin,
    Swift,
    CSharp,
    Css,
    Json,
    Yaml,
    Astro,
}

pub fn all() -> impl Iterator<Item = Lang> + Clone {
    registry::LANGS.iter().map(|spec| spec.lang)
}

impl Lang {
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Lang> {
        registry::from_extension(ext)
    }

    #[must_use]
    pub fn spec(self) -> &'static LangSpec {
        registry::get(self)
    }

    #[must_use]
    pub fn is_builtin(self) -> bool {
        self.spec().pack.is_none()
    }

    #[must_use]
    pub fn pack(self) -> Option<PackFields> {
        self.spec().pack
    }

    #[must_use]
    pub fn grammar_pack_id(self) -> Option<&'static str> {
        self.pack().map(|pack| pack.id)
    }

    #[must_use]
    pub fn grammar_symbol(self) -> Option<&'static str> {
        self.pack().map(|pack| pack.symbol)
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        self.spec().name
    }

    #[must_use]
    pub fn comment_query(self) -> &'static str {
        self.spec().comment.query()
    }

    #[must_use]
    pub fn comment_capture_names(self) -> &'static [&'static str] {
        self.spec().comment.capture_names()
    }

    /// Sub-language regions to parse with their own grammar: `(node_kind, lang)`.
    /// Astro's frontmatter is an opaque `frontmatter_js_block`, so its `//` and
    /// `/* */` comments are only reachable by re-parsing that node as TypeScript.
    #[must_use]
    pub fn injections(self) -> &'static [(&'static str, Lang)] {
        match self {
            Lang::Astro => &[("frontmatter_js_block", Lang::TypeScript)],
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const ALL_VARIANTS: [Lang; 16] = [
        Lang::TypeScript,
        Lang::Tsx,
        Lang::JavaScript,
        Lang::Python,
        Lang::Rust,
        Lang::Go,
        Lang::Toml,
        Lang::Cpp,
        Lang::Java,
        Lang::Kotlin,
        Lang::Swift,
        Lang::CSharp,
        Lang::Css,
        Lang::Json,
        Lang::Yaml,
        Lang::Astro,
    ];

    #[test]
    fn registry_covers_all_variants() {
        assert_eq!(ALL_VARIANTS.len(), LANGS.len());
        for lang in ALL_VARIANTS {
            assert_eq!(registry::get(lang).lang, lang);
        }
    }

    #[test]
    fn extension_resolution() {
        assert_eq!(Lang::from_extension("rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_extension("RS"), Some(Lang::Rust));
        assert_eq!(Lang::from_extension("tsx"), Some(Lang::Tsx));
        assert_eq!(Lang::from_extension("py"), Some(Lang::Python));
        assert_eq!(Lang::from_extension("ts"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_extension("c"), Some(Lang::Cpp));
        assert_eq!(Lang::from_extension("cpp"), Some(Lang::Cpp));
        assert_eq!(Lang::from_extension("java"), Some(Lang::Java));
        assert_eq!(Lang::from_extension("kt"), Some(Lang::Kotlin));
        assert_eq!(Lang::from_extension("kts"), Some(Lang::Kotlin));
        assert_eq!(Lang::from_extension("swift"), Some(Lang::Swift));
        assert_eq!(Lang::from_extension("cs"), Some(Lang::CSharp));
        assert_eq!(Lang::from_extension("css"), Some(Lang::Css));
        assert_eq!(Lang::from_extension("json"), Some(Lang::Json));
        assert_eq!(Lang::from_extension("yml"), Some(Lang::Yaml));
        assert_eq!(Lang::from_extension("yaml"), Some(Lang::Yaml));
        assert_eq!(Lang::from_extension("astro"), Some(Lang::Astro));
        assert_eq!(Lang::from_extension("unknown"), None);
    }

    #[test]
    fn builtin_vs_optional() {
        assert!(Lang::Python.is_builtin());
        assert!(Lang::TypeScript.is_builtin());
        assert!(Lang::Json.is_builtin());
        assert!(!Lang::Yaml.is_builtin());
        assert!(!Lang::Rust.is_builtin());
        assert!(!Lang::Cpp.is_builtin());
        assert!(!Lang::Java.is_builtin());
        assert!(!Lang::Kotlin.is_builtin());
        assert!(!Lang::Swift.is_builtin());
        assert!(!Lang::CSharp.is_builtin());
    }

    #[test]
    fn all_is_unique() {
        let langs: Vec<_> = all().collect();
        assert_eq!(langs.len(), LANGS.len());
        let set: HashSet<_> = langs.iter().copied().collect();
        assert_eq!(set.len(), langs.len());
    }

    #[test]
    fn optional_pack_count_matches_registry() {
        let n = LANGS.iter().filter(|spec| spec.pack.is_some()).count();
        assert_eq!(n, OPTIONAL_PACK_COUNT);
    }

    #[test]
    fn every_optional_lang_has_a_pack() {
        for lang in all() {
            if lang.is_builtin() {
                continue;
            }
            assert!(lang.pack().is_some(), "{lang:?} missing pack");
        }
    }
}
