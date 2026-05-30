use silence_langs::Lang;
use tree_sitter::Language;

pub fn embedded_optional(lang: Lang) -> Option<Language> {
    #[cfg(feature = "embed-optional")]
    {
        match lang {
            Lang::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
            Lang::Go => Some(tree_sitter_go::LANGUAGE.into()),
            Lang::Toml => Some(tree_sitter_toml_ng::LANGUAGE.into()),
            Lang::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
            Lang::Java => Some(tree_sitter_java::LANGUAGE.into()),
            Lang::Kotlin => Some(tree_sitter_kotlin_ng::LANGUAGE.into()),
            Lang::Swift => Some(tree_sitter_swift::LANGUAGE.into()),
            Lang::CSharp => Some(tree_sitter_c_sharp::LANGUAGE.into()),
            _ => None,
        }
    }
    #[cfg(not(feature = "embed-optional"))]
    {
        let _ = lang;
        None
    }
}
