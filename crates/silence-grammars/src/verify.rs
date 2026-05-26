use std::path::Path;

use crate::GrammarError;

// Minisign verify-only public key for grammar releases. Rotation = code
// change (intentional: tampering shows in git log). The secret half lives in
// the MINISIGN_SECRET_KEY + MINISIGN_PASSWORD GH secrets consumed by
// release.yml; only those two are sensitive.
pub const EMBEDDED_PUBLIC_KEY: Option<&str> =
    Some("RWRxct8QL3k9XRz+2LkniM1AgJ1NMxi8RYajW0C8ZbowY/4DOOBZ0Ro8");

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
