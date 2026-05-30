use super::Lang;
use crate::CommentProfile;

pub struct BuiltinLang {
    pub lang: Lang,
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub comment: CommentProfile,
}

pub const BUILTINS: &[BuiltinLang] = &[
    BuiltinLang {
        lang: Lang::TypeScript,
        name: "TypeScript",
        extensions: &["ts", "mts", "cts"],
        comment: CommentProfile::Unified,
    },
    BuiltinLang {
        lang: Lang::Tsx,
        name: "TSX",
        extensions: &["tsx"],
        comment: CommentProfile::Unified,
    },
    BuiltinLang {
        lang: Lang::JavaScript,
        name: "JavaScript",
        extensions: &["js", "mjs", "cjs", "jsx"],
        comment: CommentProfile::Unified,
    },
    BuiltinLang {
        lang: Lang::Python,
        name: "Python",
        extensions: &["py", "pyi"],
        comment: CommentProfile::Unified,
    },
];

pub fn get(lang: Lang) -> Option<&'static BuiltinLang> {
    BUILTINS.iter().find(|b| b.lang == lang)
}

pub fn from_extension(ext: &str) -> Option<Lang> {
    let e = ext.to_ascii_lowercase();
    BUILTINS
        .iter()
        .find(|b| b.extensions.contains(&e.as_str()))
        .map(|b| b.lang)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ALL;

    #[test]
    fn every_builtin_is_listed_in_all() {
        for b in BUILTINS {
            assert!(ALL.contains(&b.lang), "{:?} missing from ALL", b.lang);
        }
    }
}
