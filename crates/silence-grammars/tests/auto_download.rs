use silence_grammars::ensure;
use silence_langs::Lang;
use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;

type TestResult = Result<(), Box<dyn Error>>;
type ServerHandle = thread::JoinHandle<Result<(), String>>;

fn pack_dylib(id: &str) -> Option<PathBuf> {
    let ext = match std::env::consts::OS {
        "macos" => "dylib",
        "linux" => "so",
        _ => return None,
    };
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

fn serve_http(stream: &mut TcpStream, body: &[u8]) -> Result<(), String> {
    let mut buf = [0u8; 4096];
    let _ = stream.read(&mut buf);
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    Ok(())
}

fn spawn_http(body: Vec<u8>) -> Result<(String, ServerHandle), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();
    let handle = thread::spawn(move || -> Result<(), String> {
        let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
        serve_http(&mut stream, &body)
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
fn ensure_downloads_optional_grammar_without_embed() -> TestResult {
    let Some(pack) = pack_dylib("rust") else {
        return Err("run `cargo build -p silence-grammar-packs` before this test".into());
    };
    let body = fs::read(&pack)?;

    let home = tempfile::tempdir()?;
    let _guard = ConfigHome::set(home.path().to_path_buf());
    let (url, server) = spawn_http(body)?;

    std::env::set_var("SILENCE_GRAMMAR_TEST_URL", &url);
    let grammar = ensure(Lang::Rust)?;
    std::env::remove_var("SILENCE_GRAMMAR_TEST_URL");
    join_server(server)?;

    let query = tree_sitter::Query::new(&grammar, Lang::Rust.comment_query())?;
    assert!(query.capture_names().contains(&"line"));
    Ok(())
}
