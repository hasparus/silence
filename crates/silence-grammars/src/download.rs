use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use silence_langs::Lang;

use crate::paths;
use crate::platform::{dynamic_lib_ext, Platform};
use crate::verify;
use crate::GrammarError;

const RELEASE_BASE: &str = "https://github.com/hasparus/silence/releases/download";

pub fn cache_dir() -> PathBuf {
    paths::silence_config_dir().join("grammars")
}

pub fn cached_dylib(lang: Lang) -> Option<PathBuf> {
    let id = lang.grammar_pack_id()?;
    Some(cache_dir().join(format!("{}.{}", id, dynamic_lib_ext())))
}

pub fn download_url(lang: Lang, platform: Platform) -> Option<String> {
    let id = lang.grammar_pack_id()?;
    Some(format!(
        "{}/v{}/silence-grammar-{}-{}.{}",
        RELEASE_BASE,
        env!("CARGO_PKG_VERSION"),
        id,
        platform.asset_suffix(),
        dynamic_lib_ext()
    ))
}

pub fn ensure_on_disk(lang: Lang) -> Result<PathBuf, GrammarError> {
    let dest = cached_dylib(lang).ok_or(GrammarError::MissingPackId { lang: lang.name() })?;
    if dest.is_file() {
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
    let url =
        download_url(lang, platform).ok_or(GrammarError::MissingPackId { lang: lang.name() })?;
    eprintln!(
        "silence: installing {} grammar from release v{}…",
        lang.name(),
        env!("CARGO_PKG_VERSION")
    );
    let dest = install_from_url(lang, &url)?;
    eprintln!(
        "silence: installed {} grammar ({})",
        lang.name(),
        paths::display_home_relative(&dest)
    );
    Ok(dest)
}

pub(crate) fn install_from_url(lang: Lang, url: &str) -> Result<PathBuf, GrammarError> {
    install_from_url_with_key(lang, url, verify::EMBEDDED_PUBLIC_KEY)
}

pub(crate) fn install_from_url_with_key(
    lang: Lang,
    url: &str,
    public_key: Option<&str>,
) -> Result<PathBuf, GrammarError> {
    let dest = cached_dylib(lang).ok_or(GrammarError::MissingPackId { lang: lang.name() })?;
    if dest.is_file() {
        return Ok(dest);
    }

    let parent = dest.parent().ok_or_else(|| GrammarError::Install {
        lang: lang.name(),
        msg: format!("cache path {} has no parent", dest.display()),
    })?;
    fs::create_dir_all(parent).map_err(|e| GrammarError::Install {
        lang: lang.name(),
        msg: e.to_string(),
    })?;

    let lock = dest.with_extension("lock");
    let _lock = wait_for_lock(&lock)?;

    if dest.is_file() {
        release_lock(&lock);
        return Ok(dest);
    }

    let tmp = dest.with_extension("part");
    if let Err(e) = fetch_url(url, &tmp) {
        release_lock(&lock);
        let _ = fs::remove_file(&tmp);
        return Err(GrammarError::Install {
            lang: lang.name(),
            msg: format!("download {url}: {e}"),
        });
    }

    if let Some(key) = public_key {
        let sig_url = format!("{url}.minisig");
        let sig_text = match fetch_text(&sig_url) {
            Ok(text) => text,
            Err(e) => {
                release_lock(&lock);
                let _ = fs::remove_file(&tmp);
                return Err(GrammarError::Install {
                    lang: lang.name(),
                    msg: format!("fetch signature {sig_url}: {e}"),
                });
            }
        };
        if let Err(e) = verify::verify(&tmp, &sig_text, key) {
            release_lock(&lock);
            let _ = fs::remove_file(&tmp);
            return Err(GrammarError::Install {
                lang: lang.name(),
                msg: e.to_string(),
            });
        }
    }

    fs::rename(&tmp, &dest).map_err(|e| GrammarError::Install {
        lang: lang.name(),
        msg: e.to_string(),
    })?;
    release_lock(&lock);
    Ok(dest)
}

fn wait_for_lock(lock: &Path) -> Result<fs::File, GrammarError> {
    for _ in 0..120 {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock)
        {
            Ok(file) => return Ok(file),
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

fn release_lock(lock: &Path) {
    let _ = fs::remove_file(lock);
}

pub(crate) fn fetch_url(url: &str, dest: &Path) -> Result<(), String> {
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

fn fetch_text(url: &str) -> Result<String, String> {
    use std::io::Read;
    let resp = ureq::get(url).call().map_err(|e| e.to_string())?;
    if resp.status() != 200 {
        return Err(format!("HTTP {}", resp.status()));
    }
    let mut text = String::new();
    resp.into_body()
        .into_reader()
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    Ok(text)
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
