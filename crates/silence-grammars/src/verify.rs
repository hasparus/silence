use std::path::Path;

use crate::GrammarError;

pub const EMBEDDED_PUBLIC_KEY: Option<&str> = option_env!("SILENCE_GRAMMAR_PUBLIC_KEY");

pub fn verify(dylib: &Path, signature_text: &str, key_str: &str) -> Result<(), GrammarError> {
    let key = minisign_verify::PublicKey::from_base64(key_str).map_err(|e| GrammarError::Load {
        path: dylib.to_path_buf(),
        msg: format!("public key invalid: {e}"),
    })?;
    let signature =
        minisign_verify::Signature::decode(signature_text).map_err(|e| GrammarError::Load {
            path: dylib.to_path_buf(),
            msg: format!("signature decode: {e}"),
        })?;
    let bytes = std::fs::read(dylib).map_err(|e| GrammarError::Load {
        path: dylib.to_path_buf(),
        msg: e.to_string(),
    })?;
    key.verify(&bytes, &signature, false)
        .map_err(|e| GrammarError::Load {
            path: dylib.to_path_buf(),
            msg: format!("signature verification failed: {e}"),
        })
}
