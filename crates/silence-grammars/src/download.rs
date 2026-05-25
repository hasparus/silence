use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use silence_langs::Lang;

use crate::platform::{dynamic_lib_ext, Platform};
use crate::GrammarError;

const RELEASE_BASE: &str = "https://github.com/hasparus/silence/releases/download";

pub fn cache_dir() -> PathBuf {
    config_dir().join("grammars")
}

fn config_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from(".config/silence"), |h| PathBuf::from(h).join(".config/silence"))
}

pub fn cached_dylib(lang: Lang) -> PathBuf {
    cache_dir().join(format!(
        "{}.{}",
        lang.grammar_pack_id(),
        dynamic_lib_ext()
    ))
}

pub fn download_url(lang: Lang, platform: Platform) -> String {
    format!(
        "{}/v{}/silence-grammar-{}-{}.{}",
        RELEASE_BASE,
        env!("CARGO_PKG_VERSION"),
        lang.grammar_pack_id(),
        platform.asset_suffix(),
        dynamic_lib_ext()
    )
}

pub fn ensure_on_disk(lang: Lang) -> Result<PathBuf, GrammarError> {
    let dest = cached_dylib(lang);
    if dest.is_file() {
        return Ok(dest);
    }

    fs::create_dir_all(dest.parent().unwrap()).map_err(|e| GrammarError::Install {
        lang: lang.name(),
        msg: e.to_string(),
    })?;

    let lock = dest.with_extension("lock");
    wait_for_lock(&lock)?;

    if dest.is_file() {
        let _ = fs::remove_file(&lock);
        return Ok(dest);
    }

    let platform = Platform::detect().ok_or_else(|| GrammarError::Install {
        lang: lang.name(),
        msg: format!(
            "unsupported platform {}/{}",
            std::env::consts::ARCH,
            std::env::consts::OS
        ),
    })?;

    let url = download_url(lang, platform);
    eprintln!(
        "silence: installing {} grammar from release v{}…",
        lang.name(),
        env!("CARGO_PKG_VERSION")
    );

    let tmp = dest.with_extension("part");
    fetch_url(&url, &tmp).map_err(|e| GrammarError::Install {
        lang: lang.name(),
        msg: format!("download {url}: {e}"),
    })?;
    fs::rename(&tmp, &dest).map_err(|e| GrammarError::Install {
        lang: lang.name(),
        msg: e.to_string(),
    })?;
    let _ = fs::remove_file(&lock);

    eprintln!(
        "silence: installed {} grammar ({})",
        lang.name(),
        display_path(&dest)
    );
    Ok(dest)
}

fn display_path(path: &Path) -> String {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    if let Some(home) = home {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~{}", rest.display());
        }
    }
    path.display().to_string()
}

fn wait_for_lock(lock: &Path) -> Result<(), GrammarError> {
    for _ in 0..120 {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock)
        {
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                thread::sleep(Duration::from_millis(250));
            }
            Err(e) => {
                return Err(GrammarError::Install {
                    lang: "grammar",
                    msg: e.to_string(),
                });
            }
        }
    }
    Err(GrammarError::Install {
        lang: "grammar",
        msg: format!("timed out waiting for {}", lock.display()),
    })
}

fn fetch_url(url: &str, dest: &Path) -> Result<(), String> {
    let resp = ureq::get(url).call().map_err(|e| e.to_string())?;
    if resp.status() != 200 {
        return Err(format!("HTTP {}", resp.status()));
    }
    let mut reader = resp.into_body().into_reader();
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}
