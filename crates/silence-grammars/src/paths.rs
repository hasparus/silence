use std::path::{Path, PathBuf};

#[must_use]
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

#[must_use]
pub fn silence_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("SILENCE_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    home_dir().join(".config/silence")
}

#[must_use]
pub fn display_home_relative(path: &Path) -> String {
    let home = home_dir();
    path.strip_prefix(&home).map_or_else(
        |_| path.display().to_string(),
        |rest| format!("~/{}", rest.display()),
    )
}
