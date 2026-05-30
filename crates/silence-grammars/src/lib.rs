mod download;
mod embedded;
mod load;
pub mod paths;
mod platform;
mod verify;

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
    #[error("grammar registry mutex poisoned")]
    RegistryPoisoned,
    #[error("{lang}: missing grammar pack identifier")]
    MissingPackId { lang: &'static str },
}

static REGISTRY: OnceLock<Mutex<HashMap<Lang, Language>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<Lang, Language>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// # Errors
/// Returns an error if the registry mutex is poisoned, the grammar cannot be
/// resolved, or the loaded dylib is not a valid tree-sitter grammar.
pub fn ensure(lang: Lang) -> Result<Language, GrammarError> {
    if let Some(language) = registry()
        .lock()
        .map_err(|_| GrammarError::RegistryPoisoned)?
        .get(&lang)
    {
        return Ok(language.clone());
    }

    let language = resolve(lang)?;
    registry()
        .lock()
        .map_err(|_| GrammarError::RegistryPoisoned)?
        .insert(lang, language.clone());
    Ok(language)
}

fn resolve(lang: Lang) -> Result<Language, GrammarError> {
    if let Some(language) = builtin(lang) {
        return Ok(language);
    }

    if let Some(language) = embedded::embedded_optional(lang) {
        return Ok(language);
    }

    let symbol = lang
        .grammar_symbol()
        .ok_or(GrammarError::MissingPackId { lang: lang.name() })?;

    if let Some(path) = dev_dylib(lang) {
        return load::language_from_dylib(&path, symbol);
    }

    let path = download::ensure_on_disk(lang)?;
    load::language_from_dylib(&path, symbol)
}

fn builtin(lang: Lang) -> Option<Language> {
    match lang {
        Lang::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        Lang::Tsx | Lang::JavaScript => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        Lang::Python => Some(tree_sitter_python::LANGUAGE.into()),
        _ => None,
    }
}

fn dev_dylib(lang: Lang) -> Option<PathBuf> {
    if !cfg!(debug_assertions) || lang.is_builtin() {
        return None;
    }
    let id = lang.grammar_pack_id()?;
    let ext = platform::dynamic_lib_ext();
    let name = format!("libsilence_grammar_{id}.{ext}");
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
    fn every_grammar_loads_and_query_compiles() -> Result<(), Box<dyn std::error::Error>> {
        for &lang in ALL {
            let grammar = ensure(lang).map_err(|e| format!("{}: {e}", lang.name()))?;
            let query = tree_sitter::Query::new(&grammar, lang.comment_query())
                .map_err(|e| format!("query failed for {}: {e:?}", lang.name()))?;
            for name in lang.comment_capture_names() {
                assert!(
                    query.capture_names().contains(name),
                    "{} query must define @{name}",
                    lang.name()
                );
            }
        }
        Ok(())
    }
}
