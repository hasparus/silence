use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    DarwinAarch64,
    DarwinX86_64,
    LinuxX86_64,
}

impl Platform {
    pub fn detect() -> Option<Platform> {
        match (env::consts::ARCH, env::consts::OS) {
            ("aarch64", "macos") => Some(Platform::DarwinAarch64),
            ("x86_64", "macos") => Some(Platform::DarwinX86_64),
            ("x86_64", "linux") => Some(Platform::LinuxX86_64),
            _ => None,
        }
    }

    pub fn asset_suffix(self) -> &'static str {
        match self {
            Platform::DarwinAarch64 => "darwin-aarch64",
            Platform::DarwinX86_64 => "darwin-x86_64",
            Platform::LinuxX86_64 => "linux-x86_64",
        }
    }
}

pub fn dynamic_lib_ext() -> &'static str {
    match env::consts::OS {
        "macos" => "dylib",
        "linux" => "so",
        _ => "dll",
    }
}
