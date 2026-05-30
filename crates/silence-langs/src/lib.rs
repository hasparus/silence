mod builtins;
mod optional_packs;

pub use optional_packs::{CommentProfile, OptionalPack, PACKS as OPTIONAL_PACKS};

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
}

pub const ALL: &[Lang] = &[
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
];

impl Lang {
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Lang> {
        builtins::from_extension(ext).or_else(|| optional_packs::from_extension(ext))
    }

    #[must_use]
    pub fn is_builtin(self) -> bool {
        builtins::get(self).is_some()
    }

    #[must_use]
    pub fn optional_pack(self) -> Option<&'static OptionalPack> {
        optional_packs::get(self)
    }

    #[must_use]
    pub fn grammar_pack_id(self) -> Option<&'static str> {
        self.optional_pack().map(|pack| pack.id)
    }

    #[must_use]
    pub fn grammar_symbol(self) -> Option<&'static str> {
        self.optional_pack().map(|pack| pack.symbol)
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        lang_meta(self).0
    }

    #[must_use]
    pub fn comment_query(self) -> &'static str {
        lang_meta(self).1.query()
    }

    #[must_use]
    pub fn comment_capture_names(self) -> &'static [&'static str] {
        lang_meta(self).1.capture_names()
    }
}

fn lang_meta(lang: Lang) -> (&'static str, CommentProfile) {
    if let Some(b) = builtins::get(lang) {
        (b.name, b.comment)
    } else if let Some(p) = optional_packs::get(lang) {
        (p.name, p.comment)
    } else {
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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
        assert_eq!(Lang::from_extension("unknown"), None);
    }

    #[test]
    fn builtin_vs_optional() {
        assert!(Lang::Python.is_builtin());
        assert!(Lang::TypeScript.is_builtin());
        assert!(!Lang::Rust.is_builtin());
        assert!(!Lang::Cpp.is_builtin());
        assert!(!Lang::Java.is_builtin());
        assert!(!Lang::Kotlin.is_builtin());
        assert!(!Lang::Swift.is_builtin());
        assert!(!Lang::CSharp.is_builtin());
    }

    #[test]
    fn all_is_exhaustive_and_unique() {
        assert_eq!(ALL.len(), builtins::BUILTINS.len() + OPTIONAL_PACKS.len());
        let set: HashSet<_> = ALL.iter().copied().collect();
        assert_eq!(set.len(), ALL.len());
        for b in builtins::BUILTINS {
            assert!(set.contains(&b.lang));
        }
        for pack in OPTIONAL_PACKS {
            assert!(set.contains(&pack.lang));
        }
    }

    #[test]
    fn every_lang_has_builtin_or_optional_metadata() {
        for &lang in ALL {
            let has_builtin = builtins::get(lang).is_some();
            let has_optional = lang.optional_pack().is_some();
            assert!(
                has_builtin ^ has_optional,
                "{lang:?} must be exactly one of builtin or optional"
            );
        }
    }

    #[test]
    fn optional_pack_ids_are_unique() {
        let mut ids = OPTIONAL_PACKS
            .iter()
            .map(|pack| pack.id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), OPTIONAL_PACKS.len());
    }

    #[test]
    fn every_optional_lang_has_a_pack() {
        for &lang in ALL {
            if lang.is_builtin() {
                continue;
            }
            assert!(lang.optional_pack().is_some(), "{lang:?} missing pack");
        }
    }
}
