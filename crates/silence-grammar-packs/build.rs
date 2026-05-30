use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

type BuildResult<T> = Result<T, Box<dyn Error>>;

fn grammar_src_dir(crate_name: &str) -> BuildResult<PathBuf> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")?;
    let workspace = PathBuf::from(manifest_dir)
        .parent()
        .and_then(Path::parent)
        .ok_or("CARGO_MANIFEST_DIR has no workspace ancestor")?
        .join("Cargo.toml");
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(&workspace)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let packages = json["packages"]
        .as_array()
        .ok_or("cargo metadata: packages not an array")?;
    for pkg in packages {
        if pkg["name"] == crate_name {
            let manifest_path = pkg["manifest_path"]
                .as_str()
                .ok_or("cargo metadata: manifest_path not a string")?;
            let manifest = PathBuf::from(manifest_path);
            return manifest
                .parent()
                .map(|p| p.join("src"))
                .ok_or_else(|| format!("{manifest_path} has no parent").into());
        }
    }
    Err(format!("{crate_name} not found in workspace metadata").into())
}

fn link_shared(
    out: &Path,
    src_dir: &Path,
    parser: &Path,
    scanner: Option<&Path>,
) -> BuildResult<()> {
    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        link_shared_msvc(out, src_dir, parser, scanner)
    } else {
        link_shared_unix(out, src_dir, parser, scanner)
    }
}

fn link_shared_unix(
    out: &Path,
    src_dir: &Path,
    parser: &Path,
    scanner: Option<&Path>,
) -> BuildResult<()> {
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
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("failed to link {}", out.display()).into());
    }
    Ok(())
}

fn link_shared_msvc(
    out: &Path,
    src_dir: &Path,
    parser: &Path,
    scanner: Option<&Path>,
) -> BuildResult<()> {
    let tool = cc::Build::new()
        .cargo_metadata(false)
        .opt_level(2)
        .host(&env::var("HOST")?)
        .target(&env::var("TARGET")?)
        .get_compiler();
    let mut cmd = tool.to_command();
    cmd.arg("/nologo")
        .arg("/LD")
        .arg("/O2")
        .arg(format!("/I{}", src_dir.display()))
        .arg(format!("/Fe:{}", out.display()))
        .arg(parser);
    if let Some(scanner) = scanner {
        cmd.arg(scanner);
    }
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("failed to link {}", out.display()).into());
    }
    Ok(())
}

fn main() -> BuildResult<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let ext = match env::var("CARGO_CFG_TARGET_OS")?.as_str() {
        "macos" => "dylib",
        "linux" => "so",
        "windows" => "dll",
        os => return Err(format!("unsupported os {os}").into()),
    };
    let profile = env::var("PROFILE")?;
    let target_dir = if let Ok(dir) = env::var("CARGO_TARGET_DIR") {
        PathBuf::from(dir)
    } else {
        PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("../../target")
    };
    let dest_dir = target_dir.join(profile);

    for pack in silence_langs::OPTIONAL_PACKS {
        let src_dir = grammar_src_dir(pack.crate_name)?;
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
        )?;

        let dest = dest_dir.join(format!("libsilence_grammar_{}.{}", pack.id, ext));
        if dest != artifact {
            fs::copy(&artifact, &dest)?;
        }
    }
    Ok(())
}
