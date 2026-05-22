use serde::Deserialize;
use silence_core::{PreserveConfig, DEFAULT_PRESERVE_PATTERNS};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
pub struct FileConfig {
    #[serde(default)]
    pub preserve: Vec<String>,
    #[serde(default)]
    pub use_default_preserve: Option<bool>,
}

#[derive(Debug)]
pub struct LoadedConfig {
    pub path: Option<PathBuf>,
    file: FileConfig,
}

impl LoadedConfig {
    #[must_use]
    pub fn discover(start: &Path) -> LoadedConfig {
        if let Some(p) = find_config(start) {
            match std::fs::read_to_string(&p) {
                Ok(text) => match toml::from_str::<FileConfig>(&text) {
                    Ok(file) => {
                        return LoadedConfig {
                            path: Some(p),
                            file,
                        }
                    }
                    Err(e) => {
                        eprintln!("warning: ignoring malformed config {}: {e}", p.display());
                    }
                },
                Err(e) => {
                    eprintln!("warning: cannot read config {}: {e}", p.display());
                }
            }
        }
        LoadedConfig {
            path: None,
            file: FileConfig::default(),
        }
    }

    #[must_use]
    pub fn user_patterns(&self) -> &[String] {
        &self.file.preserve
    }

    #[must_use]
    pub fn uses_defaults(&self, no_default_preserve: bool) -> bool {
        !no_default_preserve && self.file.use_default_preserve.unwrap_or(true)
    }

    #[must_use]
    pub fn preserve(&self, no_default_preserve: bool) -> PreserveConfig {
        let use_defaults = !no_default_preserve && self.file.use_default_preserve.unwrap_or(true);

        let mut patterns = self.file.preserve.clone();
        if use_defaults {
            patterns.extend(DEFAULT_PRESERVE_PATTERNS.iter().map(|&s| s.to_string()));
        }
        PreserveConfig::with_patterns(patterns).with_directives(use_defaults)
    }
}

fn find_config(start: &Path) -> Option<PathBuf> {
    let local = start.join(".silence.toml");
    if local.is_file() {
        return Some(local);
    }
    if let Some(root) = git_root(start) {
        let at_root = root.join(".silence.toml");
        if at_root.is_file() && at_root != local {
            return Some(at_root);
        }
    }
    if let Some(home) = home_dir() {
        let global = home.join(".config").join(".silence.toml");
        if global.is_file() {
            return Some(global);
        }
    }
    None
}

fn git_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(preserve: Vec<&str>, use_default_preserve: Option<bool>) -> LoadedConfig {
        LoadedConfig {
            path: None,
            file: FileConfig {
                preserve: preserve.into_iter().map(String::from).collect(),
                use_default_preserve,
            },
        }
    }

    #[test]
    fn no_config_uses_built_in_defaults() {
        let p = loaded(vec![], None).preserve(false);
        assert!(p.should_preserve("// TODO x"));
    }

    #[test]
    fn explicit_opt_out_with_empty_list_preserves_nothing() {
        let p = loaded(vec![], Some(false)).preserve(false);
        assert!(!p.should_preserve("// TODO x"));
        assert!(!p.should_preserve("// @ts-ignore"));
    }

    #[test]
    fn user_list_merges_with_defaults_by_default() {
        let p = loaded(vec!["KEEPME"], None).preserve(false);
        assert!(p.should_preserve("// KEEPME"));
        assert!(p.should_preserve("// TODO x"));
    }

    #[test]
    fn user_list_can_replace_defaults() {
        let p = loaded(vec!["KEEPME"], Some(false)).preserve(false);
        assert!(p.should_preserve("// KEEPME"));
        assert!(!p.should_preserve("// TODO x"));
    }

    #[test]
    fn no_default_preserve_flag_keeps_user_list() {
        let p = loaded(vec!["KEEPME"], None).preserve(true);
        assert!(p.should_preserve("// KEEPME"));
        assert!(!p.should_preserve("// TODO x"));
    }
}
