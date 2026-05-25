use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Pack {
    id: &'static str,
    crate_name: &'static str,
}

const PACKS: &[Pack] = &[
    Pack {
        id: "rust",
        crate_name: "tree-sitter-rust",
    },
    Pack {
        id: "go",
        crate_name: "tree-sitter-go",
    },
    Pack {
        id: "toml",
        crate_name: "tree-sitter-toml-ng",
    },
    Pack {
        id: "cpp",
        crate_name: "tree-sitter-cpp",
    },
];

fn grammar_src_dir(crate_name: &str) -> PathBuf {
    let workspace = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Cargo.toml");
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(&workspace)
        .output()
        .expect("cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata json");
    for pkg in json["packages"].as_array().expect("packages") {
        if pkg["name"] == crate_name {
            let manifest = PathBuf::from(pkg["manifest_path"].as_str().expect("manifest_path"));
            return manifest.parent().expect("manifest dir").join("src");
        }
    }
    panic!("{crate_name} not found in workspace metadata");
}

fn link_shared(out: &Path, src_dir: &Path, parser: &Path, scanner: Option<&Path>) {
    let compiler = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let mut cmd = Command::new(compiler);
    cmd.arg("-shared")
        .arg("-fPIC")
        .arg("-std=c11")
        .arg("-O3")
        .arg(format!("-I{}", src_dir.display()))
        .arg("-o")
        .arg(out)
        .arg(parser);
    if let Some(scanner) = scanner {
        cmd.arg(scanner);
    }
    let status = cmd.status().expect("spawn cc");
    assert!(status.success(), "failed to link {out:?}");
}

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ext = match env::var("CARGO_CFG_TARGET_OS").unwrap().as_str() {
        "macos" => "dylib",
        "linux" => "so",
        "windows" => "dll",
        os => panic!("unsupported os {os}"),
    };
    let profile = env::var("PROFILE").unwrap();
    let target_dir = PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| {
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("../../target")
            .display()
            .to_string()
    }));
    let dest_dir = target_dir.join(profile);

    for pack in PACKS {
        let src_dir = grammar_src_dir(pack.crate_name);
        let parser = src_dir.join("parser.c");
        let scanner = src_dir.join("scanner.c");
        println!("cargo:rerun-if-changed={}", parser.display());
        if scanner.is_file() {
            println!("cargo:rerun-if-changed={}", scanner.display());
        }

        let artifact = out_dir.join(format!("libsilence_grammar_{}.{}", pack.id, ext));
        link_shared(
            &artifact,
            &src_dir,
            &parser,
            scanner.is_file().then_some(scanner.as_path()),
        );

        let dest = dest_dir.join(format!("libsilence_grammar_{}.{}", pack.id, ext));
        if dest != artifact {
            std::fs::copy(&artifact, &dest).expect("copy grammar dylib to target dir");
        }
    }
}
