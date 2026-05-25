mod download;
mod load;
mod platform;

use silence_langs::Lang;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use thiserror::Error;
use tree_sitter::Language;

#[derive(Debug, Error)]
pub enum GrammarError {
    #[error("grammar load {path}: {msg}")]
    Load { path: PathBuf, msg: String },
    #[error("{lang} grammar: {msg}")]
    Install { lang: &'static str, msg: String },
}

static REGISTRY: OnceLock<Mutex<HashMap<Lang, Language>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<Lang, Language>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn ensure(lang: Lang) -> Result<Language, GrammarError> {
    if let Some(language) = registry().lock().unwrap().get(&lang) {
        return Ok(language.clone());
    }

    let language = resolve(lang)?;
    registry()
        .lock()
        .unwrap()
        .insert(lang, language.clone());
    Ok(language)
}

fn resolve(lang: Lang) -> Result<Language, GrammarError> {
    if lang.is_builtin() {
        return Ok(builtin(lang));
    }

    if let Some(language) = embedded_optional(lang) {
        return Ok(language);
    }

    if let Some(path) = dev_dylib(lang) {
        return load::language_from_dylib(&path, lang.grammar_symbol());
    }

    let path = download::ensure_on_disk(lang)?;
    load::language_from_dylib(&path, lang.grammar_symbol())
}

fn builtin(lang: Lang) -> Language {
    match lang {
        Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Lang::Tsx | Lang::JavaScript => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Lang::Python => tree_sitter_python::LANGUAGE.into(),
        Lang::Rust | Lang::Go | Lang::Toml | Lang::Cpp => unreachable!("optional language"),
    }
}

fn embedded_optional(lang: Lang) -> Option<Language> {
    match lang {
        #[cfg(feature = "embed-optional")]
        Lang::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        #[cfg(feature = "embed-optional")]
        Lang::Go => Some(tree_sitter_go::LANGUAGE.into()),
        #[cfg(feature = "embed-optional")]
        Lang::Toml => Some(tree_sitter_toml_ng::LANGUAGE.into()),
        #[cfg(feature = "embed-optional")]
        Lang::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
        _ => None,
    }
}

fn dev_dylib(lang: Lang) -> Option<PathBuf> {
    if !cfg!(debug_assertions) || lang.is_builtin() {
        return None;
    }
    let ext = platform::dynamic_lib_ext();
    let name = format!("libsilence_grammar_{}.{}", lang.grammar_pack_id(), ext);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug")
        .join(name);
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use silence_langs::ALL;

    #[test]
    fn every_grammar_loads_and_query_compiles() {
        for &lang in ALL {
            let grammar = ensure(lang).unwrap_or_else(|e| panic!("{}: {e}", lang.name()));
            let query = tree_sitter::Query::new(&grammar, lang.comment_query())
                .unwrap_or_else(|e| panic!("query failed for {}: {e:?}", lang.name()));
            for name in lang.comment_capture_names() {
                assert!(
                    query.capture_names().contains(name),
                    "{} query must define @{name}",
                    lang.name()
                );
            }
        }
    }
}
