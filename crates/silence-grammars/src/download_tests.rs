use super::*;
use crate::load;
use crate::platform::Platform;
use std::error::Error;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;

type TestResult = Result<(), Box<dyn Error>>;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

struct ConfigHome {
    path: PathBuf,
    prev: Option<String>,
}

impl ConfigHome {
    fn set(path: PathBuf) -> Self {
        let prev = std::env::var("SILENCE_CONFIG_DIR").ok();
        std::env::set_var("SILENCE_CONFIG_DIR", &path);
        Self { path, prev }
    }
}

impl Drop for ConfigHome {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var("SILENCE_CONFIG_DIR", v),
            None => std::env::remove_var("SILENCE_CONFIG_DIR"),
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn pack_dylib(id: &str) -> Option<PathBuf> {
    let ext = dynamic_lib_ext();
    let name = format!("libsilence_grammar_{id}.{ext}");
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for profile in ["debug", "release"] {
        let path = base.join("target").join(profile).join(&name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

type ServerHandle = thread::JoinHandle<Result<(), String>>;

fn spawn_http(body: Vec<u8>, status: u16) -> Result<(String, ServerHandle), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let handle = thread::spawn(move || -> Result<(), String> {
        let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
        serve_http(&mut stream, status, &body)
    });
    Ok((format!("http://127.0.0.1:{port}/pack"), handle))
}

fn serve_http(stream: &mut TcpStream, status: u16, body: &[u8]) -> Result<(), String> {
    let mut buf = [0u8; 4096];
    let _ = stream.read(&mut buf);
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    Ok(())
}

fn spawn_http_pair(body: Vec<u8>, signature: Vec<u8>) -> Result<(String, ServerHandle), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let handle = thread::spawn(move || -> Result<(), String> {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = std::str::from_utf8(&buf[..n]).unwrap_or("");
            let path = req.split_whitespace().nth(1).unwrap_or("/");
            let payload: &[u8] = if path.ends_with(".minisig") {
                &signature
            } else {
                &body
            };
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            stream
                .write_all(header.as_bytes())
                .map_err(|e| e.to_string())?;
            stream.write_all(payload).map_err(|e| e.to_string())?;
        }
        Ok(())
    });
    Ok((format!("http://127.0.0.1:{port}/pack"), handle))
}

fn join_server(handle: ServerHandle) -> Result<(), String> {
    match handle.join() {
        Ok(r) => r,
        Err(_) => Err("server thread panicked".into()),
    }
}

#[test]
fn download_url_includes_version_and_platform() -> TestResult {
    let _lock = env_lock();
    for lang in silence_langs::all() {
        let Some(id) = lang.grammar_pack_id() else {
            continue;
        };
        let url =
            download_url(lang, Platform::LinuxX86_64).ok_or("language must have a pack id")?;
        assert!(url.contains(&format!("/v{}/", env!("CARGO_PKG_VERSION"))));
        assert!(url.contains(&format!("silence-grammar-{id}-linux-x86_64.")));
    }
    Ok(())
}

#[test]
fn ensure_on_disk_uses_existing_cache() -> TestResult {
    let _lock = env_lock();
    let home = tempfile::tempdir()?;
    let _guard = ConfigHome::set(home.path().to_path_buf());
    let dest = cached_dylib(Lang::Rust).ok_or("rust must have a pack id")?;
    let parent = dest.parent().ok_or("cache path has no parent")?;
    fs::create_dir_all(parent)?;
    fs::write(&dest, b"cached")?;

    let path = install_from_url(Lang::Rust, "http://127.0.0.1:1/never")?;
    assert_eq!(path, dest);
    assert_eq!(fs::read(&dest)?, b"cached");
    Ok(())
}

#[test]
fn install_from_url_fetches_and_reuses_cache() -> TestResult {
    let _lock = env_lock();
    let home = tempfile::tempdir()?;
    let _guard = ConfigHome::set(home.path().to_path_buf());
    let body = b"grammar-bytes".to_vec();
    let (url, server) = spawn_http(body.clone(), 200)?;

    let path = install_from_url_with_key(Lang::Go, &url, None)?;
    join_server(server)?;
    assert_eq!(fs::read(&path)?, body);

    let path2 = install_from_url_with_key(Lang::Go, "http://127.0.0.1:1/unreachable", None)?;
    assert_eq!(path, path2);
    assert_eq!(fs::read(&path2)?, body);
    Ok(())
}

#[test]
fn install_from_url_surfaces_http_errors() -> TestResult {
    let _lock = env_lock();
    let home = tempfile::tempdir()?;
    let _guard = ConfigHome::set(home.path().to_path_buf());
    let (url, server) = spawn_http(Vec::new(), 404)?;

    let err = install_from_url(Lang::Toml, &url)
        .err()
        .ok_or("expected error")?;
    join_server(server)?;
    let msg = err.to_string();
    assert!(msg.contains("404"), "{msg}");
    Ok(())
}

#[test]
fn install_with_invalid_signature_fails() -> TestResult {
    let _lock = env_lock();
    let home = tempfile::tempdir()?;
    let _guard = ConfigHome::set(home.path().to_path_buf());

    let kp = minisign::KeyPair::generate_unencrypted_keypair()?;
    let pubkey = kp.pk.to_base64();
    let body = b"grammar-bytes-tampered".to_vec();
    let sig_box = minisign::sign(None, &kp.sk, b"different-content".as_slice(), None, None)?;
    let sig_text = sig_box.into_string();

    let (url, server) = spawn_http_pair(body, sig_text.into_bytes())?;
    let err = install_from_url_with_key(Lang::Toml, &url, Some(&pubkey))
        .err()
        .ok_or("expected verification error")?;
    join_server(server)?;
    assert!(
        err.to_string().contains("signature verification failed"),
        "{err}"
    );
    let cached = cached_dylib(Lang::Toml).ok_or("toml must have a pack id")?;
    assert!(!cached.exists(), "tampered dylib must not be cached");
    Ok(())
}

#[test]
fn downloaded_pack_loads_and_query_compiles() -> TestResult {
    let _lock = env_lock();
    let Some(pack) = pack_dylib("rust") else {
        eprintln!("skip downloaded_pack_loads: build silence-grammar-packs first");
        return Ok(());
    };
    let bytes = fs::read(&pack)?;

    let home = tempfile::tempdir()?;
    let _guard = ConfigHome::set(home.path().to_path_buf());
    let (url, server) = spawn_http(bytes, 200)?;

    let path = install_from_url_with_key(Lang::Rust, &url, None)?;
    join_server(server)?;

    let symbol = Lang::Rust.grammar_symbol().ok_or("rust grammar symbol")?;
    let grammar = load::language_from_dylib(&path, symbol)?;
    let query = tree_sitter::Query::new(&grammar, Lang::Rust.comment_query())?;
    assert!(query.capture_names().contains(&"line"));
    assert!(query.capture_names().contains(&"block"));
    Ok(())
}

#[test]
fn signed_pack_round_trips_through_install_and_load() -> TestResult {
    let _lock = env_lock();
    let Some(pack) = pack_dylib("rust") else {
        eprintln!("skip signed_pack_round_trips: build silence-grammar-packs first");
        return Ok(());
    };
    let bytes = fs::read(&pack)?;

    let home = tempfile::tempdir()?;
    let _guard = ConfigHome::set(home.path().to_path_buf());

    let kp = minisign::KeyPair::generate_unencrypted_keypair()?;
    let pubkey = kp.pk.to_base64();
    let sig_box = minisign::sign(None, &kp.sk, bytes.as_slice(), None, None)?;
    let sig_text = sig_box.into_string();

    let (url, server) = spawn_http_pair(bytes, sig_text.into_bytes())?;
    let path = install_from_url_with_key(Lang::Rust, &url, Some(&pubkey))?;
    join_server(server)?;

    let symbol = Lang::Rust.grammar_symbol().ok_or("rust grammar symbol")?;
    let grammar = load::language_from_dylib(&path, symbol)?;
    let query = tree_sitter::Query::new(&grammar, Lang::Rust.comment_query())?;
    assert!(query.capture_names().contains(&"line"));
    assert!(query.capture_names().contains(&"block"));
    Ok(())
}
