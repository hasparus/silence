use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    DarwinAarch64,
    DarwinX86_64,
    LinuxX86_64,
    LinuxAarch64,
    WindowsX86_64,
    WindowsAarch64,
}

impl Platform {
    pub fn detect() -> Option<Platform> {
        match (env::consts::ARCH, env::consts::OS) {
            ("aarch64", "macos") => Some(Platform::DarwinAarch64),
            ("x86_64", "macos") => Some(Platform::DarwinX86_64),
            ("x86_64", "linux") => Some(Platform::LinuxX86_64),
            ("aarch64", "linux") => Some(Platform::LinuxAarch64),
            ("x86_64", "windows") => Some(Platform::WindowsX86_64),
            ("aarch64", "windows") => Some(Platform::WindowsAarch64),
            _ => None,
        }
    }

    pub fn asset_suffix(self) -> &'static str {
        match self {
            Platform::DarwinAarch64 => "darwin-aarch64",
            Platform::DarwinX86_64 => "darwin-x86_64",
            Platform::LinuxX86_64 => "linux-x86_64",
            Platform::LinuxAarch64 => "linux-aarch64",
            Platform::WindowsX86_64 => "windows-x86_64",
            Platform::WindowsAarch64 => "windows-aarch64",
        }
    }
}

pub fn dynamic_lib_ext() -> &'static str {
    match env::consts::OS {
        "macos" => "dylib",
        "windows" => "dll",
        _ => "so",
    }
}
