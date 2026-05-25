use std::mem;
use std::path::Path;

use libloading::{Library, Symbol};
use tree_sitter::Language;

use crate::GrammarError;

pub fn language_from_dylib(path: &Path, symbol: &str) -> Result<Language, GrammarError> {
    let library = unsafe { Library::new(path) }.map_err(|e| GrammarError::Load {
        path: path.to_path_buf(),
        msg: e.to_string(),
    })?;
    let language = unsafe {
        let language_fn: Symbol<unsafe extern "C" fn() -> Language> = library
            .get(symbol.as_bytes())
            .map_err(|e| GrammarError::Load {
                path: path.to_path_buf(),
                msg: format!("symbol {symbol}: {e}"),
            })?;
        language_fn()
    };
    mem::forget(library);
    Ok(language)
}
